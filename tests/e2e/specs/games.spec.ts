import { test, expect, type Page } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CAMERA, OWNER, PLAYER, STAFF, addCourt, advanceClock, boxExec, colourAt, completeSetup, courtCode, freshBox, isMagenta, powerCycle,
  probe, signIn, startCameras, waitCourtStatus,
} from './helpers';

test.describe.configure({ mode: 'serial' });

const LOGO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../fixtures/logo-magenta.png');
let code = '';

const tokenOf = async (page: Page, selector: string): Promise<string> => (await page.locator(selector).first().getAttribute('href'))!.split('/').pop()!;
const files = (token: string): { wide: string; vertical: string } => ({
  wide: `/data/clips/${token}/wide.mp4`,
  vertical: `/data/clips/${token}/vertical.mp4`,
});

async function owner(page: Page): Promise<void> {
  await signIn(page, OWNER.username, OWNER.password);
  await expect(page).toHaveURL(`${STAFF}/console`);
}

async function setJudge(page: Page, on: boolean): Promise<void> {
  await owner(page);
  await page.goto(`${STAFF}/console/settings`);
  const box = page.getByLabel(/Make highlights/);
  if (on) await box.check();
  else await box.uncheck();
  await page.getByRole('button', { name: 'Save settings' }).click();
  await expect(page.getByRole('status')).toHaveText('Settings saved.');
}

test('the court page shows its parts in order; Start offers posting permission unticked (FR-17, FR-11)', async ({ page, browser }) => {
  test.setTimeout(240_000); // a fresh box from nothing
  await freshBox();
  await startCameras('cam-4');
  await completeSetup(page, OWNER, ['00:00', '23:59'], LOGO);
  await page.goto(`${STAFF}/console/settings`);
  await page.getByLabel('Logo corner').selectOption('bottom-right');
  await page.getByRole('button', { name: 'Save settings' }).click();
  await addCourt(page, 'Court 6', CAMERA(4));
  code = await courtCode(page, 'Court 6');
  await waitCourtStatus(page, 'Court 6', /^Recording since/);

  const phone = await (await browser.newContext()).newPage();
  await phone.goto(`${PLAYER}/c/${code}`);
  const order = await phone.locator('img.venue-logo, h1, [data-court-status], .btn-clip-that, #game-h, #clips-h').evaluateAll((els) => els.map((e) => e.id || e.className || e.tagName.toLowerCase()));
  expect(order).toEqual(['venue-logo', 'court', 'status status-ok', 'btn-clip-that', 'game-h', 'clips-h']);
  const permission = phone.getByLabel('The venue may post our highlights on its pages');
  await expect(permission).toBeVisible();
  await expect(permission).not.toBeChecked();
  // Venue network only: players are never asked for an address; their videos download here. local-only:
  await expect(phone.locator('input[type=email], [name=email], form[action$="/emails"]')).toHaveCount(0); // local-only: no address field
  expect((await phone.request.post(`${PLAYER}/c/${code}/emails`, { form: { email: 'pia@example.com' }, headers: { Origin: PLAYER } })).status()).toBe(404); // local-only: no route
});

test('a game runs on every phone, and Stop yields the game video and highlights (FR-10, FR-12, FR-14)', async ({ browser }) => {
  test.setTimeout(400_000);
  const phoneA = await (await browser.newContext()).newPage();
  const phoneB = await (await browser.newContext()).newPage();
  await phoneA.goto(`${PLAYER}/c/${code}`);
  await phoneA.getByRole('button', { name: 'Start game' }).click();
  await expect(phoneA.locator('[data-game-running]')).toHaveText(/^Recording since \d{1,2}:\d{2} [ap]m$/);
  await phoneB.goto(`${PLAYER}/c/${code}`);
  await expect(phoneB.locator('[data-game-running]')).toHaveText(/^Recording since/);
  await expect(phoneB.getByRole('button', { name: 'Stop game' })).toBeVisible();
  await expect(phoneB.getByRole('button', { name: 'Start game' })).toHaveCount(0);

  // A tap during the game belongs to the game.
  await phoneA.goto(`${PLAYER}/c/${code}`);
  await phoneA.getByRole('button', { name: 'Clip that' }).click();
  await expect(phoneA.locator('[data-clip-state="ready"]')).toHaveCount(1, { timeout: 90_000 });

  await phoneA.waitForTimeout(45_000); // about 60 s of play: three 20 s rally cycles
  await phoneB.goto(`${PLAYER}/c/${code}`);
  await phoneB.getByRole('button', { name: 'Stop game' }).click();
  await expect(phoneB.getByRole('button', { name: 'Start game' })).toBeVisible();
  await expect(phoneB.locator('[data-last-game]')).toBeVisible();
  await expect(phoneB.locator('[data-highlights-coming]')).toHaveText(/^Highlights coming — about \d+ min$/);
  // The page refreshes the game area by itself until everything is made.
  await expect(phoneB.locator('[data-game-video]')).toBeVisible({ timeout: 180_000 });
  await expect(phoneB.locator('[data-highlights]')).toBeVisible({ timeout: 240_000 });
  await expect(phoneB.locator('[data-highlights-coming]')).toHaveCount(0);
  await expect(phoneB.getByRole('heading', { name: 'Highlights' })).toBeVisible();

  const gameToken = await tokenOf(phoneB, '[data-game-video]');
  const game = files(gameToken);
  const g = probe(game.wide);
  expect(g.seconds).toBeGreaterThan(50);
  expect([g.width, g.height]).toEqual([640, 360]);
  // One copy of the game video, the one players download. local-only: no smaller copy to send anywhere
  expect(boxExec(`ls /data/clips/${gameToken}`).trim().split('\n').filter((f) => f.endsWith('.mp4'))).toEqual(['wide.mp4']);

  const reel = files(await tokenOf(phoneB, '[data-reel]'));
  const singles = await phoneB.locator('[data-highlight]').evaluateAll((as) => as.map((a) => a.getAttribute('href')!.split('/').pop()!));
  expect(singles.length).toBe(5);
  const lengths = singles.map((t) => probe(files(t).wide).seconds);
  for (const s of lengths) {
    expect(s).toBeGreaterThan(4);
    expect(s).toBeLessThanOrEqual(20.1);
  }
  expect(lengths[0], 'best first: the longest rally leads').toBeGreaterThanOrEqual(lengths[4] - 0.5);
  const r = probe(reel.wide);
  expect(Math.abs(r.seconds - lengths.reduce((a, b) => a + b, 0))).toBeLessThan(2);
  for (const t of singles) {
    const v = probe(files(t).vertical);
    expect([v.width, v.height]).toEqual([360, 640]);
  }
});

test('the logo sits in the chosen corner of a clip, a highlight and a game video, every shape, and nothing has sound (FR-6, FR-2)', async ({ page }) => {
  await page.goto(`${PLAYER}/c/${code}`);
  const clip = files(await tokenOf(page, '[data-clip-state="ready"] a.clip-link'));
  const game = files(await tokenOf(page, '[data-game-video]'));
  const highlight = files(await tokenOf(page, '[data-highlight]'));
  const reel = files(await tokenOf(page, '[data-reel]'));
  // Bottom-right: wide 116×46 and vertical 130×52, 11 px from the edges (media.rs place()).
  const wideSpot: [number, number, number, number] = [40, 20, 551, 316];
  const verticalSpot: [number, number, number, number] = [40, 20, 264, 593];
  for (const [label, file, spot] of [
    ['clip wide', clip.wide, wideSpot], ['clip vertical', clip.vertical, verticalSpot],
    ['highlight wide', highlight.wide, wideSpot], ['highlight vertical', highlight.vertical, verticalSpot],
    ['reel wide', reel.wide, wideSpot], ['reel vertical', reel.vertical, verticalSpot],
    ['game video', game.wide, wideSpot],
  ] as const) {
    expect(isMagenta(colourAt(file, ...spot)), `${label}: logo bottom-right`).toBe(true);
    expect(isMagenta(colourAt(file, 40, 20, 24, 24)), `${label}: top-left clear`).toBe(false);
    const p = probe(file);
    expect(p.streams, `${label}: no audio although the camera sends it`).toEqual(['video']);
    expect(p.startsOnKeyframe, `${label}: starts on a keyframe`).toBe(true);
  }
});

test('posting permission is stored with the game and shown in the library (FR-11, FR-31)', async ({ page, browser }) => {
  test.setTimeout(240_000);
  await owner(page);
  await page.goto(`${STAFF}/console/clips?kind=game`);
  await expect(page.locator('tbody tr').first().locator('td:nth-child(7)')).toHaveText('No');
  await page.goto(`${STAFF}/console/clips?may_post=1`);
  await expect(page.getByRole('heading', { name: 'No clips match these filters' })).toBeVisible();

  await setJudge(page, false); // quicker; FR-16 is checked here too
  const phone = await (await browser.newContext()).newPage();
  await phone.goto(`${PLAYER}/c/${code}`);
  await phone.getByLabel('The venue may post our highlights on its pages').check();
  await phone.getByRole('button', { name: 'Start game' }).click();
  await phone.waitForTimeout(8_000);
  await phone.getByRole('button', { name: 'Stop game' }).click();
  await expect(phone.locator('[data-game-video]')).toBeVisible({ timeout: 120_000 });
  await expect(phone.locator('[data-highlights-coming]'), 'judge off: no highlight message').toHaveCount(0);
  await expect(phone.locator('[data-highlights]')).toHaveCount(0);
  await page.goto(`${STAFF}/console/clips?may_post=1`);
  await expect(page.locator('tbody tr')).toHaveCount(1);
  await expect(page.locator('tbody tr td:nth-child(4)')).toHaveText('Game video');
  await expect(page.locator('tbody tr td:nth-child(7)')).toHaveText('Yes');
});

test('a game stops by itself after 2 hours, or at closing time when that comes first (FR-10, GV-9, GV-21)', async ({ page, browser }) => {
  test.setTimeout(240_000);
  const phone = await (await browser.newContext()).newPage();
  await phone.goto(`${PLAYER}/c/${code}`);
  await phone.getByRole('button', { name: 'Start game' }).click();
  const since = (await phone.locator('[data-game-running]').textContent())!.replace('Recording since ', '');
  await expect(phone.getByText(/It stops by itself at/)).toBeVisible();
  await advanceClock(page.request, 2 * 3600_000 + 1_000);
  await expect.poll(async () => {
    await phone.goto(`${PLAYER}/c/${code}`);
    return phone.getByRole('button', { name: 'Start game' }).count();
  }, { timeout: 20_000 }).toBe(1);
  const [h, m] = since.replace(/ [ap]m/, '').split(':').map(Number);
  const pm = since.endsWith('pm');
  const end = new Date(Date.UTC(2000, 0, 1, (h % 12) + (pm ? 12 : 0) + 2, m));
  const endLabel = `${((end.getUTCHours() + 11) % 12) + 1}:${String(end.getUTCMinutes()).padStart(2, '0')} ${end.getUTCHours() >= 12 ? 'pm' : 'am'}`;
  await expect(phone.locator('[data-last-game] h3').first()).toHaveText(`Last game, ${since} – ${endLabel}`);

  // Closing in 2 minutes: a game started now stops at closing, not 2 h later.
  await owner(page);
  const now = new Date(Date.parse((await (await page.request.post(`${STAFF}/__test/clock`, { form: { ms: '0' } })).text())) + 8 * 3600_000);
  const close = new Date(now.getTime() + 2 * 60_000);
  const hhmm = (d: Date): string => d.toISOString().slice(11, 16);
  await page.goto(`${STAFF}/console/settings`);
  await page.getByLabel('Opens').fill('00:00');
  await page.getByLabel('Closes').fill(hhmm(close));
  await page.getByRole('button', { name: 'Save settings' }).click();
  await phone.goto(`${PLAYER}/c/${code}`);
  await phone.getByRole('button', { name: 'Start game' }).click();
  const closeLabel = `${((close.getUTCHours() + 11) % 12) + 1}:${hhmm(close).slice(3)} ${close.getUTCHours() >= 12 ? 'pm' : 'am'}`;
  await expect(phone.getByText(`It stops by itself at ${closeLabel}.`)).toBeVisible();
  await advanceClock(page.request, 2 * 60_000 + 5_000);
  await expect.poll(async () => {
    await phone.goto(`${PLAYER}/c/${code}`);
    return (await phone.locator('[data-last-game] h3').first().textContent()) ?? '';
  }, { timeout: 20_000 }).toContain(`– ${closeLabel}`);
  await page.goto(`${STAFF}/console/settings`);
  await page.getByLabel('Closes').fill('23:59');
  await page.getByRole('button', { name: 'Save settings' }).click();
  await waitCourtStatus(page, 'Court 6', /^Recording since/, 30_000);
});

test('after a power cut everything made through the UI is still there, recording resumes, and the cut game keeps its footage (FR-13, FR-56)', async ({ page, browser }) => {
  test.setTimeout(300_000);
  await owner(page);
  // A staff account, a hold and a settings change, made through the UI.
  await page.goto(`${STAFF}/console/staff/new`);
  await page.getByLabel('Name', { exact: true }).fill('Ken Desk');
  await page.getByLabel('Username', { exact: true }).fill('ken');
  await page.getByRole('button', { name: /Add staff/ }).click();
  const temp = (await page.locator('[data-temp-password]').textContent())!.trim();
  await page.goto(`${STAFF}/console/settings`);
  await page.getByLabel(/Venue contact/).fill('Front desk 0917 555 0133');
  await page.getByRole('button', { name: 'Save settings' }).click();
  const manila = new Date(Date.parse((await (await page.request.post(`${STAFF}/__test/clock`, { form: { ms: '0' } })).text())) + 8 * 3600_000);
  const hm = (d: Date): string => d.toISOString().slice(11, 16);
  await page.goto(`${STAFF}/console/footage?court=1&date=${manila.toISOString().slice(0, 10)}&from=${hm(new Date(manila.getTime() - 120_000))}&to=${hm(manila)}`);
  await page.getByRole('button', { name: 'Hold footage' }).click();
  await expect(page.getByRole('status')).toContainText('Footage held');
  await page.goto(`${STAFF}/console/clips`);
  const clipsBefore = Number(await page.locator('[data-total]').textContent());

  const phone = await (await browser.newContext()).newPage();
  await phone.goto(`${PLAYER}/c/${code}`);
  await phone.getByRole('button', { name: 'Start game' }).click();
  const started = Date.now();
  await phone.waitForTimeout(30_000);
  const cut = await powerCycle();
  const back = Date.now();

  await owner(page);
  await waitCourtStatus(page, 'Court 6', /^Recording since/, 60_000);
  expect(Date.now() - back, 'recording resumes within 60 s of start-up').toBeLessThan(60_000);
  await page.goto(`${STAFF}/console/clips`);
  expect(Number(await page.locator('[data-total]').textContent())).toBeGreaterThanOrEqual(clipsBefore);
  await page.goto(`${STAFF}/console/settings`);
  await expect(page.getByLabel(/Venue contact/)).toHaveValue('Front desk 0917 555 0133');
  await page.goto(`${STAFF}/console/footage`);
  await expect(page.locator('tr[data-hold]').first()).toContainText('Court 6');
  const ken = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await signIn(ken, 'ken', temp);
  await expect(ken.getByLabel('New password')).toBeVisible();

  // The game the power cut interrupted became a game video of the footage before the cut.
  await phone.goto(`${PLAYER}/c/${code}`);
  await expect(phone.getByRole('button', { name: 'Start game' })).toBeVisible();
  await expect(phone.locator('[data-game-video]')).toBeVisible({ timeout: 120_000 });
  const video = probe(files(await tokenOf(phone, '[data-game-video]')).wide);
  const played = (cut - started) / 1000;
  expect(video.seconds, 'keeps the footage before the cut (at most one 10 s fragment lost)').toBeGreaterThan(played - 12);
  expect(video.seconds).toBeLessThan(played + 2);
  expect(boxExec('ls /data/footage/court-1 | wc -l').trim()).not.toBe('0');
});

import { test, expect, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import jsQR from 'jsqr';
import { PNG } from 'pngjs';
import { CAMERA, OWNER, PLAYER, STAFF, addCourt, advanceClock, boxExec, completeSetup, courtCode, freshBox, restartBox, signIn, startCameras, waitCourtStatus } from './helpers';

test.describe.configure({ mode: 'serial' });

const LOGO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../fixtures/logo-magenta.png');
const logoBytes = fs.readFileSync(LOGO);

async function owner(page: Page): Promise<void> {
  await signIn(page, OWNER.username, OWNER.password);
  await expect(page).toHaveURL(`${STAFF}/console`);
}

async function logText(page: Page): Promise<string> {
  await page.goto(`${STAFF}/console/log`);
  return (await page.locator('table').textContent()) ?? '';
}

async function statements(page: Page): Promise<number> {
  return Number(await (await page.request.get(`${STAFF}/__test/statements`)).text());
}

async function settingsForm(page: Page): Promise<void> {
  await page.goto(`${STAFF}/console/settings`);
}

test('first use: the library explains how clips arrive; signs and help work (FR-37, FR-39)', async ({ page }) => {
  await freshBox();
  await startCameras('cam-3');
  await completeSetup(page, OWNER, ['00:00', '23:59'], LOGO);
  await page.goto(`${STAFF}/console/clips`);
  await expect(page.getByRole('heading', { name: 'No clips yet — players tap Clip that on the court page' })).toBeVisible();
  await expect(page.locator('[data-illustration="scan-code"]')).toBeVisible();

  await page.goto(`${STAFF}/console/help`);
  await expect(page.locator('[data-support-phone]')).toHaveText('0917 555 0100');
  await expect(page.getByText('7 days, about 6 am–11 pm')).toBeVisible();
  await expect(page.getByRole('link', { name: 'Call support' })).toHaveAttribute('href', 'tel:09175550100');
  await expect(page.locator('[data-fingerprint]')).toHaveText(/^([0-9A-F]{2}:){31}[0-9A-F]{2}$/);

  await addCourt(page, 'Court 4', CAMERA(3));
  await addCourt(page, 'Court 5', '');
  await settingsForm(page);
  await page.getByLabel(/Venue contact/).fill('Front desk 0917 123 4567');
  await page.getByRole('button', { name: 'Save settings' }).click();
  await expect(page.getByRole('status')).toHaveText('Settings saved.');

  const codes = { 'Court 4': await courtCode(page, 'Court 4'), 'Court 5': await courtCode(page, 'Court 5') };
  await page.goto(`${STAFF}/console/courts`);
  await page.getByRole('link', { name: 'Print court signs' }).click();
  await expect(page.locator('article.sign')).toHaveCount(2);
  for (const [court, code] of Object.entries(codes)) {
    const sign = page.locator(`article.sign[data-sign="${code}"]`);
    await expect(sign.getByRole('heading', { name: court })).toBeVisible();
    await expect(sign).toContainText(`Court code ${code}`);
    for (const text of ['Join the venue Wi-Fi first.', 'This court is recorded (no sound)', 'Footage kept 4 hours, clips 3 days.', 'Under 18? Ask a parent first.', 'Questions: Front desk 0917 123 4567']) {
      await expect(sign).toContainText(text);
    }
    // The printed code really opens this court's page.
    const png = PNG.sync.read(await sign.locator('.sign-qr svg').screenshot());
    const decoded = jsQR(new Uint8ClampedArray(png.data), png.width, png.height);
    expect(decoded?.data).toBe(`${PLAYER}/c/${code}`);
    const player = await page.context().browser()!.newPage();
    await player.goto(decoded!.data);
    await expect(player.getByRole('heading', { level: 1 })).toHaveText(court);
    await player.close();
  }
  // One printed page per court.
  await page.goto(`${STAFF}/console/signs`);
  const pdf = await page.pdf({ format: 'A4', printBackground: true });
  expect(pdf.toString('latin1').match(/\/Type\s*\/Page(?!s)/g)?.length).toBe(2);
});

test('logo uploads refuse > 2 MB, renamed executables, SVG and other formats; nothing is stored (FR-33, SN-4)', async ({ page }) => {
  await owner(page);
  const bad = [
    { name: 'big.png', mimeType: 'image/png', buffer: Buffer.concat([logoBytes.subarray(0, 8), Buffer.alloc(3 * 1024 * 1024)]), msg: 'larger than 2 MB' },
    { name: 'logo.png', mimeType: 'image/png', buffer: Buffer.from('MZ\x90\x00\x03\x00\x00\x00\x04\x00\x00\x00\xff\xff', 'latin1'), msg: 'must be a PNG or JPEG' },
    { name: 'logo.svg', mimeType: 'image/svg+xml', buffer: Buffer.from('<svg xmlns="http://www.w3.org/2000/svg"><script>alert(1)</script></svg>'), msg: 'must be a PNG or JPEG' },
    { name: 'logo.gif', mimeType: 'image/gif', buffer: Buffer.from('GIF89a\x01\x00\x01\x00', 'latin1'), msg: 'must be a PNG or JPEG' },
  ];
  for (const file of bad) {
    for (const field of ['Venue logo', 'Sponsor logo']) {
      await settingsForm(page);
      await page.getByLabel(new RegExp(field)).setInputFiles(file);
      await page.getByLabel('Venue name').fill('Renamed by a refused upload');
      await page.getByRole('button', { name: 'Save settings' }).click();
      await expect(page.getByRole('alert')).toContainText(file.msg);
      await expect(page.getByRole('alert')).toContainText('Nothing was saved.');
    }
  }
  const stored = await page.request.get(`${STAFF}/console/logo`);
  expect(Buffer.from(await stored.body()).equals(logoBytes), 'the original logo is unchanged').toBe(true);
  await settingsForm(page);
  await expect(page.getByLabel('Venue name')).toHaveValue(OWNER.venue);
  expect(await page.locator('#sponsor-hint').textContent()).not.toContain('Remove');
});

test('settings save, survive a restart and take effect on the next clip: corner and clip lengths (FR-33, FR-6)', async ({ page }) => {
  test.setTimeout(240_000);
  await owner(page);
  await settingsForm(page);
  await page.getByLabel('Logo corner').selectOption('bottom-left');
  await page.getByLabel('Seconds before the tap').fill('10');
  await page.getByLabel('Seconds after the tap').fill('2');
  await page.getByLabel(/Upload cap/).fill('25');
  await page.getByLabel(/Make highlights/).uncheck();
  await page.getByLabel('Privacy notice shown to players').fill('Test venue notice.\n\nSecond paragraph.');
  await page.getByRole('button', { name: 'Save settings' }).click();
  await expect(page.getByRole('status')).toHaveText('Settings saved.');
  await restartBox();
  await owner(page);
  await settingsForm(page);
  await expect(page.getByLabel('Logo corner')).toHaveValue('bottom-left');
  await expect(page.getByLabel('Seconds before the tap')).toHaveValue('10');
  await expect(page.getByLabel('Seconds after the tap')).toHaveValue('2');
  await expect(page.getByLabel(/Upload cap/)).toHaveValue('25');
  await expect(page.getByLabel(/Make highlights/)).not.toBeChecked();

  const player = await page.context().browser()!.newPage();
  await player.goto(`${PLAYER}/privacy`);
  await expect(player.getByText('Second paragraph.')).toBeVisible();
  const code = await courtCode(page, 'Court 4');
  await waitCourtStatus(page, 'Court 4', /^Recording since/);
  await player.waitForTimeout(12_000);
  await player.goto(`${PLAYER}/c/${code}`);
  await player.getByRole('button', { name: 'Clip that' }).click();
  await expect(player.locator('[data-saving-text]')).toHaveText(/Saving… ready in [12] s/);
  await expect(player.locator('[data-clip-state="ready"]')).toHaveCount(1, { timeout: 60_000 });
  const token = (await player.locator('[data-clip-state="ready"] a.clip-link').getAttribute('href'))!.split('/').pop();
  const wide = `/data/clips/${token}/wide.mp4`;
  const seconds = Number(boxExec(`ffprobe -v error -show_entries format=duration -of csv=p=0 ${wide}`).trim());
  expect(seconds).toBeGreaterThan(10.5);
  expect(seconds).toBeLessThan(13.5);
  // Bottom-left on 640×360: logo 116×46 at 11 px from the left and bottom (media.rs place()).
  const rgb = boxExec(`ffmpeg -v error -ss 1 -i ${wide} -frames:v 1 -vf crop=40:20:30:316,scale=1:1:flags=area -f rawvideo -pix_fmt rgb24 - | od -An -tu1`).trim().split(/\s+/).map(Number);
  expect(rgb[0] > 190 && rgb[1] < 70 && rgb[2] > 190, `logo bottom-left, got ${rgb}`).toBe(true);
  await player.close();
});

test('the Owner corrects the box time (FR-38)', async ({ page }) => {
  await owner(page);
  await settingsForm(page);
  const shown = (await page.locator('[data-box-time]').textContent())!;
  const wanted = new Date(Date.now() + 8 * 3600_000 + 3 * 60_000); // Manila wall time + 3 min
  const value = wanted.toISOString().slice(0, 16);
  await page.getByLabel('Correct time').fill(value);
  await page.getByRole('button', { name: 'Correct the time' }).click();
  await expect(page.getByRole('status')).toHaveText('Box time corrected.');
  const after = (await page.locator('[data-box-time]').textContent())!;
  expect(after).not.toBe(shown);
  const hhmm = (d: Date): string => `${((d.getUTCHours() + 11) % 12) + 1}:${String(d.getUTCMinutes()).padStart(2, '0')}`;
  expect(after).toContain(hhmm(wanted));
  expect(await logText(page)).toContain('corrected box time');
  await restartBox();
  await owner(page);
  await settingsForm(page);
  expect(await page.locator('[data-box-time]').textContent()).toContain(hhmm(wanted));
});

test('footage: find, hold against the 4-hour overwrite, export, record missing footage; every step logged (FR-32, GV-18)', async ({ page }) => {
  test.setTimeout(300_000);
  await owner(page);
  const now = new Date(Date.parse((await (await page.request.post(`${STAFF}/__test/clock`, { form: { ms: '0' } })).text())));
  const manila = (d: Date): Date => new Date(d.getTime() + 8 * 3600_000);
  const hm = (d: Date): string => manila(d).toISOString().slice(11, 16);
  const day = manila(now).toISOString().slice(0, 10);
  await page.goto(`${STAFF}/console/footage`);
  await page.getByLabel('Court', { exact: true }).selectOption({ label: 'Court 4' });
  await page.getByLabel('Date').fill(day);
  await page.getByLabel('From').fill(hm(new Date(now.getTime() - 3 * 60_000)));
  await page.getByLabel('To').fill(hm(new Date(now.getTime() + 60_000)));
  await page.getByRole('button', { name: 'Find footage' }).click();
  await expect(page.getByRole('heading', { name: 'Footage found' })).toBeVisible();
  await page.getByRole('button', { name: 'Hold footage' }).click();
  await expect(page.getByRole('status')).toContainText('Footage held');
  const releases = new Date(manila(now).getTime() + 60 * 86400_000);
  await expect(page.locator('tr[data-hold] td.num').first()).toHaveText(`${releases.getUTCDate()} ${releases.toLocaleString('en-GB', { month: 'short', timeZone: 'UTC' })} ${releases.getUTCFullYear()}`);

  const held = boxExec('ls /data/footage/court-1/').trim().split('\n').filter(Boolean);
  await advanceClock(page.request, 4 * 3600_000 + 5 * 60_000);
  await page.waitForTimeout(70_000); // the overwrite runs every minute
  const left = boxExec('ls /data/footage/court-1/').trim().split('\n');
  const heldStart = now.getTime() - 3 * 60_000;
  const protectedFiles = held.filter((f) => Number(f.replace('.mp4', '')) >= heldStart - 10_000);
  expect(protectedFiles.length).toBeGreaterThan(0);
  for (const f of protectedFiles) expect(left, `${f} is held`).toContain(f);
  const unprotected = held.filter((f) => Number(f.replace('.mp4', '')) < heldStart - 15_000);
  for (const f of unprotected) expect(left, `${f} is not held`).not.toContain(f);

  await page.goto(`${STAFF}/console/footage`);
  const [download] = await Promise.all([page.waitForEvent('download'), page.getByRole('link', { name: /^Export / }).click()]);
  expect(download.suggestedFilename()).toMatch(/^court-4-\d{8}-\d{4}-footage\.mp4$/);
  const probe = boxExec('ffprobe -v error -show_entries stream=codec_type:format=duration -of csv=p=0 /data/exports/hold-1.mp4').trim();
  expect(probe).toMatch(/^video/m);
  expect(probe).not.toContain('audio');

  await page.goto(`${STAFF}/console/footage?court=1&date=${day}&from=00:00&to=00:30`);
  await expect(page.getByRole('heading', { name: 'No footage for that time' })).toBeVisible();
  await page.getByRole('button', { name: 'Record that it no longer exists' }).click();
  await expect(page.getByRole('status')).toHaveText('Recorded in the activity log.');
  await page.getByRole('button', { name: /^Release / }).click();
  await expect(page.getByRole('status')).toContainText('Hold released');

  const log = await logText(page);
  for (const entry of ['held footage', 'exported footage', 'footage requested but no longer exists', 'released hold']) expect(log).toContain(entry);
});

test('library: 50 per page, filters with Clear, no-results state, keep, posting warning, delete with confirmation, bounded queries (FR-31, FR-35)', async ({ page }) => {
  test.setTimeout(240_000);
  await owner(page);
  const seeded = await page.request.post(`${STAFF}/__test/seed-clips`, { form: { court: '1', n: '612' } });
  expect(seeded.ok()).toBe(true);
  await page.goto(`${STAFF}/console/clips`);
  const total = Number(await page.locator('[data-total]').textContent());
  expect(total).toBeGreaterThanOrEqual(613);
  await expect(page.locator('caption')).toContainText(`1–50 of ${total}`);
  await expect(page.locator('tbody tr')).toHaveCount(50);
  await expect(page.locator('thead th[scope="col"]')).toHaveCount(8);
  await page.getByRole('link', { name: 'Next 50' }).click();
  await expect(page.locator('caption')).toContainText(`51–100 of ${total}`);

  // Bounded queries: page 1 of 600+ rows costs the same statements as a filtered page of a few rows.
  const cost = async (url: string): Promise<number> => {
    const before = await statements(page);
    expect((await page.request.get(url)).ok()).toBe(true);
    return (await statements(page)) - before;
  };
  const big = await cost(`${STAFF}/console/clips`);
  const small = await cost(`${STAFF}/console/clips?kind=tap&kept=1`);
  expect(big).toBe(small);
  expect(big).toBeLessThanOrEqual(8);

  await page.getByLabel('Type').selectOption('game');
  await page.getByRole('button', { name: 'Show clips' }).click();
  await expect(page.locator('caption')).toContainText('matching clips');
  await expect(page.locator('tbody td:nth-child(4)').first()).toHaveText('Game video');
  await expect(page.getByRole('link', { name: 'Clear filters' })).toBeVisible();
  await page.getByLabel('Date').fill('2030-01-01');
  await page.getByRole('button', { name: 'Show clips' }).click();
  await expect(page.getByRole('heading', { name: 'No clips match these filters' })).toBeVisible();
  await page.getByRole('link', { name: 'Clear filters' }).first().click();
  await expect(page).toHaveURL(`${STAFF}/console/clips`);

  // The real clip made in the settings test (11–13 s; every seeded row is exactly 30 s and has no files):
  // keep, download both shapes, posting warning, delete.
  let id: string | null = null;
  for (let p = 1; p <= 13 && !id; p++) {
    await page.goto(`${STAFF}/console/clips?kind=tap&page=${p}`);
    const lengths = await page.locator('tbody tr').evaluateAll((rows) => rows.map((r) => [r.getAttribute('data-clip'), r.children[4]?.textContent?.trim()]));
    id = lengths.find(([, len]) => len !== '30 s')?.[0] ?? null;
  }
  expect(id, 'the real clip is in the library').not.toBeNull();
  const realRow = page.locator(`tr[data-clip="${id}"]`);
  await realRow.getByRole('button', { name: /^Keep / }).click();
  await expect(page.getByRole('status')).toHaveText('Kept for 1 year.');
  await page.goto(`${STAFF}/console/clips?kind=tap&kept=1`);
  const keptRow = page.locator(`tr[data-clip="${id}"]`);
  // Kept items go at the first 03:00 clean-up at least 1 year after Keep (GV-12), so the date can be a day later.
  const kept = new Date(Date.parse((await (await page.request.post(`${STAFF}/__test/clock`, { form: { ms: '0' } })).text())) + 8 * 3600_000);
  const inAYear = new Date(Date.UTC(kept.getUTCFullYear() + 1, kept.getUTCMonth(), kept.getUTCDate(), kept.getUTCHours(), kept.getUTCMinutes()));
  const cleanup = new Date(Date.UTC(inAYear.getUTCFullYear(), inAYear.getUTCMonth(), inAYear.getUTCDate(), 3));
  if (cleanup < inAYear) cleanup.setUTCDate(cleanup.getUTCDate() + 1);
  const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];
  await expect(keptRow).toContainText(`Kept until ${cleanup.getUTCDate()} ${MONTHS[cleanup.getUTCMonth()]} ${cleanup.getUTCFullYear()}`);
  for (const name of [/^Download Court 4/, /^Download vertical Court 4/]) {
    const [d] = await Promise.all([page.waitForEvent('download'), keptRow.getByRole('link', { name }).click()]);
    expect(d.suggestedFilename()).toMatch(/^court-4-\d{8}-\d{6}-(wide|vertical)\.mp4$/);
  }
  await keptRow.getByRole('link', { name: /^Download for posting/ }).click();
  await expect(page.getByRole('heading', { name: "The players didn't allow posting" })).toBeVisible();
  const [posted] = await Promise.all([page.waitForEvent('download'), page.getByRole('link', { name: 'Download for posting anyway' }).click()]);
  expect(posted.suggestedFilename()).toMatch(/\.mp4$/);

  await page.goto(`${STAFF}/console/clips/${id}/delete`);
  await expect(page.getByRole('heading', { name: 'Delete this clip?' })).toBeVisible();
  await expect(page.getByText(/Court 4 · .* · Clip is removed .* It can't be recovered\./)).toBeVisible();
  await page.getByRole('button', { name: 'Delete' }).click();
  await expect(page.getByRole('status')).toHaveText('Clip deleted.');
  await expect(page.locator(`tr[data-clip="${id}"]`)).toHaveCount(0);
  expect(boxExec(`ls /data/clips/ | wc -l`).trim()).toBe('0');

  const log = await logText(page);
  for (const entry of ['kept', 'downloaded for posting', 'players did NOT allow posting', 'deleted', 'changed settings', 'signed in']) expect(log).toContain(entry);
  expect(log).toContain(OWNER.username);
});

test('the activity log exports to CSV with formulas defused and no email addresses (FR-35)', async ({ page }) => {
  await owner(page);
  await settingsForm(page);
  await page.getByLabel('Venue name').fill('=HYPERLINK("http://evil.example")');
  await page.getByRole('button', { name: 'Save settings' }).click();
  await settingsForm(page);
  await page.getByLabel('Venue name').fill(OWNER.venue);
  await page.getByRole('button', { name: 'Save settings' }).click();
  await page.goto(`${STAFF}/console/log`);
  const [download] = await Promise.all([page.waitForEvent('download'), page.getByRole('link', { name: 'Export CSV' }).click()]);
  const csv = fs.readFileSync(await download.path(), 'utf8');
  expect(csv.split('\r\n')[0]).toBe('time,who,action,item,detail');
  expect(csv).toContain('"\'=HYPERLINK(""http://evil.example"")"');
  expect(csv).not.toMatch(/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/);
  for (const entry of ['signed in', 'changed settings', 'held footage', 'deleted']) expect(csv).toContain(entry);
});

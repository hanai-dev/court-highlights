import { test, expect, type Browser, type Page } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CAMERA, CLOUD, OWNER, PLAYER, STAFF, addCourt, advanceClock, boxExec, clearMail, cloudSql, cloudUp, compose, completeSetup, courtCode,
  freshBox, msUntilManila, registerBox, sentMail, signIn, startCameras, waitCourtStatus, type Mail,
} from './helpers';

test.describe.configure({ mode: 'serial' });

const LOGO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../fixtures/logo-magenta.png');
let code = '';
let firstMail: Mail | undefined;

const DAYS = ['Sun', 'Mon', 'Tue', 'Wed', 'Thu', 'Fri', 'Sat'];
const MONTHS = ['Jan', 'Feb', 'Mar', 'Apr', 'May', 'Jun', 'Jul', 'Aug', 'Sep', 'Oct', 'Nov', 'Dec'];

/** "available until Fri 23 Oct 2026, 3:00 am": the first 03:00 (Manila) at least 72 h after `madeMs` (GV-10). */
function expiryText(madeMs: number): string {
  const m = new Date(madeMs + 8 * 3600_000 + 72 * 3600_000);
  const c = new Date(Date.UTC(m.getUTCFullYear(), m.getUTCMonth(), m.getUTCDate(), 3, 0, 0));
  if (c < m) c.setUTCDate(c.getUTCDate() + 1);
  return `available until ${DAYS[c.getUTCDay()]} ${c.getUTCDate()} ${MONTHS[c.getUTCMonth()]} ${c.getUTCFullYear()}, 3:00 am`;
}

const linkIn = (mail: Mail, re: RegExp): string => mail.text.match(re)![0];
const pageLink = (mail: Mail): string => linkIn(mail, /http:\/\/localhost:18090\/v\/[0-9a-f]{64}(?=\n)/);

async function owner(page: Page): Promise<void> {
  await signIn(page, OWNER.username, OWNER.password);
  await expect(page).toHaveURL(`${STAFF}/console`);
}

async function phone(browser: Browser): Promise<Page> {
  const p = await (await browser.newContext()).newPage();
  await p.goto(`${PLAYER}/c/${code}`);
  return p;
}

async function addEmail(p: Page, address: string): Promise<void> {
  await p.getByLabel('Email address').fill(address);
  await p.getByRole('button', { name: 'Add' }).click();
  await expect(p.locator('.chip').filter({ hasText: `${address[0]}••••@` }).first()).toBeVisible();
}

async function tap(p: Page): Promise<string> {
  await p.goto(`${PLAYER}/c/${code}`);
  const before = await p.locator('[data-clip-state="ready"]').count();
  await p.getByRole('button', { name: 'Clip that' }).click();
  await expect(p.locator('[data-clip-state="ready"]')).toHaveCount(before + 1, { timeout: 60_000 });
  return (await p.locator('[data-clip-state="ready"] a.clip-link').first().getAttribute('href'))!.split('/').pop()!;
}

async function settings(page: Page, fill: Record<string, string>, judge?: boolean): Promise<void> {
  await owner(page);
  await page.goto(`${STAFF}/console/settings`);
  for (const [label, value] of Object.entries(fill)) await page.getByLabel(label).fill(value);
  if (judge !== undefined) await (judge ? page.getByLabel(/Make highlights/).check() : page.getByLabel(/Make highlights/).uncheck());
  await page.getByRole('button', { name: 'Save settings' }).click();
  await expect(page.getByRole('status')).toHaveText('Settings saved.');
}

test('email is offered only once the box is registered with the online service (FR-48, FR-27)', async ({ page, browser }) => {
  await freshBox();
  await cloudUp();
  await clearMail();
  await startCameras('cam-4');
  await completeSetup(page, OWNER, ['00:00', '23:59'], LOGO);
  await settings(page, { 'Seconds before the tap': '10', 'Seconds after the tap': '2' });
  await addCourt(page, 'Court 1', CAMERA(4));
  code = await courtCode(page, 'Court 1');
  await waitCourtStatus(page, 'Court 1', /^Recording since/);

  const p = await phone(browser);
  await expect(p.getByText("Download your videos here — email isn't available at this venue.")).toBeVisible();

  const key = registerBox(OWNER.venue);
  expect(key).toMatch(/^[0-9a-f]{64}$/);
  await page.goto(`${STAFF}/console/settings`);
  await page.getByLabel('Box key').fill(key);
  await page.getByRole('button', { name: 'Save box key' }).click();
  await expect(page.getByRole('status')).toHaveText('Box key saved. The box connects within a minute.');
  await expect.poll(async () => {
    await page.goto(`${STAFF}/console/settings`);
    return page.locator('[data-cloud-status]').textContent();
  }, { timeout: 30_000 }).toMatch(/^Connected\. Last check-in/);
  expect(await page.content(), 'the key is never shown back').not.toContain(key);
  await p.goto(`${PLAYER}/c/${code}`);
  await expect(p.getByText('Start a game to send its videos by email.')).toBeVisible();
});

test('each address gets one email per game with links and expiry; hidden clips are not in it; then nobody keeps the addresses (FR-20, FR-23, FR-9, FR-14)', async ({ browser }) => {
  test.setTimeout(500_000);
  const p = await phone(browser);
  await p.getByRole('button', { name: 'Start game' }).click();
  await addEmail(p, 'ana@example.com');
  await addEmail(p, 'ben@example.com');
  await p.waitForTimeout(12_000);
  const tapAt = Date.now();
  const kept = await tap(p);
  await p.waitForTimeout(8_000);
  const hidden = await tap(p);
  await p.goto(`${PLAYER}/c/${code}/clips/${hidden}`);
  await p.getByRole('button', { name: 'Hide from this page' }).click();
  await p.waitForTimeout(20_000);
  await p.getByRole('button', { name: 'Stop game' }).click();
  await expect(p.locator('[data-highlights-coming]')).toBeVisible();

  await expect.poll(async () => (await sentMail()).length, { timeout: 300_000, intervals: [3000] }).toBe(2);
  const mails = await sentMail();
  expect(mails.map((m) => m.to).sort()).toEqual([['ana@example.com'], ['ben@example.com']]);
  const m = mails[0];
  firstMail = mails.find((x) => x.to[0] === 'ana@example.com');
  expect(m.subject).toMatch(new RegExp(`^Your videos from ${OWNER.venue} — Court 1 game, `));
  expect(m.text).toContain(expiryText(tapAt));
  expect(m.text).toContain("This isn't me — don't email me again: http://localhost:18090/never/");
  expect(m.html).toContain('See and download your videos');

  // The link page: game video (720p), the reel and single highlights, the kept clip — not the hidden one.
  const viewer = await (await browser.newContext()).newPage();
  await viewer.goto(pageLink(m));
  const labels = await viewer.locator('li[data-item] h2').allTextContents();
  expect(labels).toContain('Game video, Court 1');
  expect(labels).toContain('Highlights reel, Court 1');
  expect(labels.filter((l) => l.startsWith('Highlight, Court 1')).length).toBeGreaterThan(0);
  expect(labels.filter((l) => l.startsWith('Clip, Court 1')).length, 'the hidden clip is never sent').toBe(1);

  // Downloads come straight from storage and are the box's files.
  const gameToken = (await p.locator('[data-game-video]').getAttribute('href'))!.split('/').pop()!;
  const gameItem = viewer.locator('li[data-item]').filter({ hasText: 'Game video, Court 1' }).getByRole('link', { name: 'Download' });
  const gameFile = await viewer.request.get(new URL((await gameItem.getAttribute('href'))!, CLOUD).toString());
  expect(gameFile.status()).toBe(200);
  const gameBytes = await gameFile.body();
  expect(gameBytes.subarray(4, 8).toString()).toBe('ftyp');
  expect(gameBytes.length, 'the 720p copy, not the full-quality video').toBe(Number(boxExec(`stat -c %s /data/clips/${gameToken}/email.mp4`).trim()));
  const clipItem = viewer.locator('li[data-item]').filter({ has: viewer.locator('h2', { hasText: /^Clip, Court 1/ }) }).getByRole('link', { name: 'Download' });
  const clipFile = await viewer.request.get(new URL((await clipItem.getAttribute('href'))!, CLOUD).toString());
  expect((await clipFile.body()).length).toBe(Number(boxExec(`stat -c %s /data/clips/${kept}/wide.mp4`).trim()));

  // Nobody keeps the addresses: not the court page, not the box's files, not the service's database.
  await p.goto(`${PLAYER}/c/${code}`);
  await expect(p.getByText('Start a game to send its videos by email.')).toBeVisible();
  expect(boxExec('cat /data/box.db /data/box.db-wal 2>/dev/null | grep -c "example.com" || true').trim()).toBe('0');
  expect(compose('exec', '-T', 'db', 'pg_dump', '-U', 'postgres', '--data-only', 'ch').includes('example.com')).toBe(false);
  expect(cloudSql('SELECT count(*) FROM never_link')).toBe('2');
});

test('"don\'t email me again" blocks that address for every later game; guessed links return nothing (FR-25, SN-6)', async ({ browser, page }) => {
  test.setTimeout(400_000);
  const never = linkIn(firstMail!, /http:\/\/localhost:18090\/never\/[0-9a-f]{64}/);
  const v = await (await browser.newContext()).newPage();
  await v.goto(never);
  await v.getByRole('button', { name: "Don't email me again" }).click();
  await expect(v.getByRole('heading', { name: "We won't email you again" })).toBeVisible();
  const guessed = '0'.repeat(64);
  expect((await v.request.get(`${CLOUD}/v/${guessed}`)).status()).toBe(404);
  expect((await v.request.get(`${CLOUD}/never/${guessed}`)).status()).toBe(404);

  // A hard bounce puts dee on the email service's suppression list, which the online service turned on at start.
  await fetch('http://localhost:18025/bounced', { method: 'POST', body: JSON.stringify({ address: 'dee.bounced@example.com' }) });

  await settings(page, {}, false); // no highlights: quicker
  await clearMail();
  const p = await phone(browser);
  await p.getByRole('button', { name: 'Start game' }).click();
  await addEmail(p, 'ana@example.com');
  await addEmail(p, 'ben@example.com');
  await addEmail(p, 'dee.bounced@example.com');
  await p.waitForTimeout(8_000);
  await p.getByRole('button', { name: 'Stop game' }).click();
  await expect.poll(async () => (await sentMail()).length, { timeout: 180_000, intervals: [3000] }).toBe(1);
  await p.waitForTimeout(5_000);
  const mails = await sentMail();
  expect(mails.map((m) => m.to[0])).toEqual(['ben@example.com']);
  const suppression = await (await fetch('http://localhost:18025/suppression')).json();
  expect(suppression.reasons).toEqual(['BOUNCE', 'COMPLAINT']);
  expect(suppression.dropped).toContain('dee.bounced@example.com');
});

test('a clip outside a game offers Email me this (FR-21)', async ({ browser }) => {
  test.setTimeout(200_000);
  await clearMail();
  const p = await phone(browser);
  const token = await tap(p);
  await p.goto(`${PLAYER}/c/${code}/clips/${token}`);
  await expect(p.getByRole('heading', { name: 'Email me this' })).toBeVisible();
  await addEmail(p, 'cara@example.com');
  await expect.poll(async () => (await sentMail()).length, { timeout: 120_000, intervals: [3000] }).toBe(1);
  const [m] = await sentMail();
  expect(m.to).toEqual(['cara@example.com']);
  expect(m.subject).toMatch(/— Court 1 clip, /);
  const viewer = await (await browser.newContext()).newPage();
  await viewer.goto(pageLink(m));
  await expect(viewer.locator('li[data-item]')).toHaveCount(1);
});

test('with the line down, clips and on-court downloads still work; back up, items go clips first, then 720p game videos, oldest game first, within the cap; a deleted clip is withdrawn (FR-22, FR-26)', async ({ browser, page }) => {
  test.setTimeout(500_000);
  await settings(page, { 'Upload cap for sending videos (Mbit/s)': '2' });
  await clearMail();
  const firstNew = Number(cloudSql('SELECT COALESCE(MAX(id), 0) FROM item'));
  compose('stop', 'cloud');

  const games: { clip: string; video: string }[] = [];
  for (const who of ['dan@example.com', 'eve@example.com']) {
    const p = await phone(browser);
    await p.getByRole('button', { name: 'Start game' }).click();
    await addEmail(p, who);
    await p.waitForTimeout(6_000);
    const clip = await tap(p);
    // On-court download works without the online service.
    expect((await p.request.get(`${PLAYER}/c/${code}/clips/${clip}/wide.mp4?download=1`)).status()).toBe(200);
    await p.goto(`${PLAYER}/c/${code}`);
    await p.getByRole('button', { name: 'Stop game' }).click();
    await expect(p.locator('[data-game-video]')).toBeVisible({ timeout: 120_000 });
    games.push({ clip, video: (await p.locator('[data-game-video]').getAttribute('href'))!.split('/').pop()! });
  }
  expect(await sentMail()).toHaveLength(0);

  const sendingFrom = (await advanceClock(page.request, 0)).getTime();
  await cloudUp();
  await expect.poll(async () => (await sentMail()).length, { timeout: 240_000, intervals: [3000] }).toBe(2);
  // Recording carried on while the box sent everything (PERF-UPLOAD-CAP): no stretch over 2 s without footage.
  await page.waitForTimeout(12_000); // the segment that spans the end of sending closes
  const gap = Number(await (await page.request.get(`${STAFF}/__test/max-gap?since_ms=${sendingFrom}`)).text());
  expect(gap, 'longest gap in footage while sending (ms)').toBeLessThanOrEqual(2000);
  const kinds = cloudSql(`SELECT kind FROM item WHERE id > ${firstNew} ORDER BY uploaded_at`).split('\n');
  expect(kinds, 'clips of both games first, then the game videos').toEqual(['tap', 'tap', 'game', 'game']);
  const refs = cloudSql(`SELECT box_ref FROM item WHERE id > ${firstNew} AND kind = 'game' ORDER BY uploaded_at`).split('\n');
  expect(Number(refs[0].replace('item-', '')), 'oldest game first').toBeLessThan(Number(refs[1].replace('item-', '')));

  // Within the 2 Mbit/s cap (250 kB/s), allowing 15 % for timing.
  const uploads = compose('logs', 'box-a').split('\n').filter((l) => l.includes('"message":"uploaded"') && /"cap_kbps":2000[,}]/.test(l));
  expect(uploads.length).toBeGreaterThanOrEqual(4);
  for (const u of uploads) {
    const f = JSON.parse(u.slice(u.indexOf('{'))).fields as { bytes: number; ms: number };
    expect(f.bytes / (f.ms / 1000), `upload of ${f.bytes} bytes in ${f.ms} ms`).toBeLessThanOrEqual(250_000 * 1.15);
  }

  // Deleting a sent clip in the library withdraws its online copy.
  const mail = (await sentMail()).find((m) => m.to[0] === 'eve@example.com')!;
  const viewer = await (await browser.newContext()).newPage();
  await viewer.goto(pageLink(mail));
  const clipLink = new URL((await viewer.locator('li[data-item]').filter({ has: viewer.locator('h2', { hasText: /^Clip, Court 1/ }) }).getByRole('link', { name: 'Download' }).getAttribute('href'))!, CLOUD).toString();
  expect((await viewer.request.get(clipLink)).status()).toBe(200);
  await owner(page);
  await page.goto(`${STAFF}/console/clips?kind=tap`);
  const rowId = await page.locator('tbody tr').first().getAttribute('data-clip');
  await page.goto(`${STAFF}/console/clips/${rowId}/delete`);
  await page.getByRole('button', { name: 'Delete' }).click();
  await expect.poll(async () => (await viewer.request.get(clipLink)).status(), { timeout: 30_000, intervals: [2000] }).toBe(410);
  await viewer.goto(pageLink(mail));
  await expect(viewer.locator('li[data-item]').filter({ has: viewer.locator('h2', { hasText: /^Clip, Court 1/ }) })).toHaveCount(0);
});

test('past the expiry moment the email link says the videos have expired and the box copy goes at the 3:00 am clean-up (FR-24, FR-45)', async ({ browser, page }) => {
  test.setTimeout(200_000);
  const v = await (await browser.newContext()).newPage();
  await v.goto(pageLink(firstMail!));
  await expect(v.locator('li[data-item]').first()).toBeVisible();
  await v.request.post(`${CLOUD}/__test/clock`, { form: { ms: String(4 * 24 * 3600_000) } });
  const res = await v.goto(pageLink(firstMail!));
  expect(res!.status()).toBe(410);
  await expect(v.getByRole('heading', { name: 'These videos have expired' })).toBeVisible();

  const before = boxExec('ls /data/clips | wc -l').trim();
  expect(Number(before)).toBeGreaterThan(0);
  await advanceClock(page.request, 72 * 3600_000);
  await advanceClock(page.request, await msUntilManila(page.request, 3, 0, 1));
  await expect.poll(() => boxExec('ls /data/clips | wc -l').trim(), { timeout: 60_000, intervals: [3000] }).toBe('0');
});

import { test, expect, type Browser, type Page } from '@playwright/test';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { CAMERA, OWNER, PLAYER, STAFF, compose, freshBox, startCameras } from './helpers';

test.describe.configure({ mode: 'serial' });
// Three recording boxes are heavy: never leave them running after this file.
test.afterAll(() => {
  compose('--profile', 'venue', 'stop', 'venue-a', 'venue-b', 'venue-c');
});

const LOGO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../fixtures/logo-magenta.png');
const BOXES = ['a', 'b', 'c'] as const;
type Box = (typeof BOXES)[number];
const staffUrl = (b: Box): string => `https://localhost:${18543 + BOXES.indexOf(b)}`;
const playerUrl = (b: Box): string => `http://localhost:${18180 + BOXES.indexOf(b)}`;
const vc = (...args: string[]): string => compose('--profile', 'venue', ...args);
const codes: Record<string, string> = {};

async function waitUp(b: Box): Promise<void> {
  for (let i = 0; i < 90; i++) {
    try {
      if ((await fetch(`${playerUrl(b)}/healthz`)).ok) return;
    } catch {
      /* starting */
    }
    await new Promise((r) => setTimeout(r, 1000));
  }
  throw new Error(`venue box ${b} did not start`);
}

async function staffPage(browser: Browser, b: Box): Promise<Page> {
  const p = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await signInOn(p, b);
  return p;
}

async function signInOn(p: Page, b: Box, username = OWNER.username, password = OWNER.password): Promise<void> {
  await p.goto(`${staffUrl(b)}/console/signin`);
  // A box that hasn't received the venue yet shows first-time setup instead: nothing to sign in to.
  if (!p.url().startsWith(`${staffUrl(b)}/console/signin`)) return;
  await p.getByLabel('Username').fill(username);
  await p.getByLabel('Password').fill(password);
  await p.getByRole('button', { name: 'Sign in' }).click();
}

async function addCourtOn(p: Page, b: Box, name: string): Promise<void> {
  await p.goto(`${staffUrl(b)}/console/courts/new`);
  await p.getByLabel('Court name').fill(name);
  await p.getByLabel('Camera stream address').fill(CAMERA(3));
  await p.getByRole('button', { name: 'Save court' }).click();
  await expect(p.getByRole('status')).toContainText('Saved');
}

/** Polls a box's console home until the court's status matches. */
async function courtStatusOn(p: Page, b: Box, court: string, pattern: RegExp, timeout = 60_000): Promise<void> {
  await expect
    .poll(async () => {
      await p.goto(`${staffUrl(b)}/console`);
      return (await p.locator(`tr[data-court="${court}"] [data-court-status]`).textContent({ timeout: 1000 }).catch(() => '')) ?? '';
    }, { timeout, intervals: [2000] })
    .toMatch(pattern);
}

test('a venue of three boxes shares staff, settings and courts; any box shows the whole venue (FR-41, FR-42)', async ({ browser }) => {
  test.setTimeout(300_000);
  vc('down', '-v', '--remove-orphans');
  vc('up', '-d', 'venue-a', 'venue-b', 'venue-c');
  for (const b of BOXES) await waitUp(b);
  await startCameras('cam-3');

  // Setup happens once, on Box A; B and C receive the venue and the Owner within a minute.
  const a = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await a.goto(`${staffUrl('a')}/setup`);
  await a.getByLabel('Venue name').fill(OWNER.venue);
  await a.getByLabel(/Venue logo/).setInputFiles(LOGO);
  await a.getByLabel('Opens').fill('00:00');
  await a.getByLabel('Closes').fill('23:59');
  await a.getByLabel('Your name').fill(OWNER.name);
  await a.getByLabel('Owner username').fill(OWNER.username);
  await a.getByLabel('Owner password').fill(OWNER.password);
  await a.getByRole('button', { name: 'Continue' }).click();
  await expect(a).toHaveURL(`${staffUrl('a')}/console`);
  const b = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  const c = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  for (const [p, box] of [[b, 'b'], [c, 'c']] as const) {
    await expect.poll(async () => {
      await signInOn(p, box);
      return p.url();
    }, { timeout: 60_000, intervals: [3000] }).toBe(`${staffUrl(box)}/console`);
  }

  // Each box records one court; every console lists all three with their box.
  // A 12-court venue, four courts per box: Court n is recorded by box a, b, c in turn (Courts 1–3 on A, B, C).
  const pages = { a, b, c };
  const all = Array.from({ length: 12 }, (_, i) => `Court ${i + 1}`);
  for (const [i, court] of all.entries()) await addCourtOn(pages[BOXES[i % 3]], BOXES[i % 3], court);
  for (const [p, box] of [[a, 'a'], [b, 'b'], [c, 'c']] as const) {
    for (const court of all) await courtStatusOn(p, box, court, /^Recording since/);
    await expect(p.locator('tbody tr[data-court]')).toHaveCount(12);
  }
  await a.goto(`${staffUrl('a')}/console`);
  for (const court of ['Court 1', 'Court 2', 'Court 3']) {
    codes[court] = ((await a.locator(`tr[data-court="${court}"] [data-court-code]`).textContent()) ?? '').trim();
  }
  await expect(a.locator('tr[data-court="Court 2"] td').last()).toHaveText('box-b');
  await expect(a.locator('tr[data-box="box-c"]')).toContainText('Yes');

  // A sign printed on Box A sends players of Court 2 to Box B, and Box A's link for it redirects there too.
  await a.goto(`${staffUrl('a')}/console/signs`);
  await expect(a.locator(`article.sign[data-sign="${codes['Court 2']}"] .sign-url`)).toHaveText(`${playerUrl('b')}/c/${codes['Court 2']}`);
  const phone = await browser.newPage();
  await phone.goto(`${playerUrl('a')}/c/${codes['Court 2']}`);
  await expect(phone).toHaveURL(`${playerUrl('b')}/c/${codes['Court 2']}`);
  await expect(phone.getByRole('heading', { level: 1 })).toHaveText('Court 2');
});

test('a settings change on one box reaches the others within a minute (FR-33, FR-42)', async ({ browser }) => {
  const b = await staffPage(browser, 'b');
  await b.goto(`${staffUrl('b')}/console/settings`);
  await b.getByLabel('Seconds before the tap').fill('12');
  await b.getByLabel(/Venue contact/).fill('Desk 0917 000 1111');
  await b.getByRole('button', { name: 'Save settings' }).click();
  await expect(b.getByRole('status')).toHaveText('Settings saved.');
  for (const box of ['a', 'c'] as const) {
    const p = await staffPage(browser, box);
    await expect.poll(async () => {
      await p.goto(`${staffUrl(box)}/console/settings`);
      return p.getByLabel('Seconds before the tap').inputValue();
    }, { timeout: 60_000, intervals: [3000] }).toBe('12');
    await expect(p.getByLabel(/Venue contact/)).toHaveValue('Desk 0917 000 1111');
  }
});

test('deactivating a staff member ends their session on every box; 5 wrong passwords lock them on every box (FR-28, FR-29)', async ({ browser }) => {
  test.setTimeout(240_000);
  const a = await staffPage(browser, 'a');
  const addStaff = async (name: string, username: string): Promise<string> => {
    await a.goto(`${staffUrl('a')}/console/staff/new`);
    await a.getByLabel('Name', { exact: true }).fill(name);
    await a.getByLabel('Username', { exact: true }).fill(username);
    await a.getByRole('button', { name: /Add staff/ }).click();
    return ((await a.locator('[data-temp-password]').textContent()) ?? '').trim();
  };
  const kenTemp = await addStaff('Ken Desk', 'ken');
  const ken = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await expect.poll(async () => {
    await signInOn(ken, 'c', 'ken', kenTemp);
    return ken.url();
  }, { timeout: 60_000, intervals: [3000] }).toContain('/console/account');
  await ken.getByLabel('Current password').fill(kenTemp);
  await ken.getByLabel('New password').fill('ken-own-password-2026');
  await ken.getByRole('button', { name: 'Change password' }).click();
  await ken.goto(`${staffUrl('c')}/console`);
  await expect(ken).toHaveURL(`${staffUrl('c')}/console`);

  await a.goto(`${staffUrl('a')}/console/staff`);
  await a.getByRole('link', { name: /Ken Desk/ }).click();
  await a.getByRole('button', { name: /^Deactivate/ }).click();
  await expect.poll(async () => {
    await ken.goto(`${staffUrl('c')}/console`);
    return ken.url();
  }, { timeout: 60_000, intervals: [3000] }).toBe(`${staffUrl('c')}/console/signin`);

  const pamTemp = await addStaff('Pam Desk', 'pam');
  // Wait until Box B has Pam's account (a wrong-password alert alone can't tell "unknown" from "wrong").
  const ownerB = await staffPage(browser, 'b');
  await expect.poll(async () => {
    await ownerB.goto(`${staffUrl('b')}/console/staff`);
    return ownerB.getByText('Pam Desk').count();
  }, { timeout: 60_000, intervals: [3000] }).toBe(1);
  const pam = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  for (let i = 0; i < 5; i++) await signInOn(pam, 'b', 'pam', 'wrong-password-123');
  await signInOn(pam, 'b', 'pam', pamTemp);
  await expect(pam, 'locked on Box B').toHaveURL(`${staffUrl('b')}/console/signin`);
  // The lock reaches Box C: even the right password is refused there.
  await expect.poll(async () => {
    await signInOn(pam, 'c', 'pam', pamTemp);
    return pam.url();
  }, { timeout: 60_000, intervals: [5000] }).toBe(`${staffUrl('c')}/console/signin`);
  await expect(pam.getByRole('alert')).toContainText('locked');
});

test('the clip library lists all three boxes; keep, download and delete work on any box\'s clips from one console (FR-31, FR-41)', async ({ browser }) => {
  test.setTimeout(300_000);
  const tokens: Record<string, string> = {};
  for (const [court, box] of [['Court 1', 'a'], ['Court 2', 'b'], ['Court 3', 'c']] as const) {
    const p = await browser.newPage();
    await p.goto(`${playerUrl(box)}/c/${codes[court]}`);
    await p.waitForTimeout(13_000);
    await p.getByRole('button', { name: 'Clip that' }).click();
    await expect(p.locator('[data-clip-state="ready"]')).toHaveCount(1, { timeout: 90_000 });
    tokens[court] = (await p.locator('[data-clip-state="ready"] a.clip-link').getAttribute('href'))!.split('/').pop()!;
  }
  const a = await staffPage(browser, 'a');
  await a.goto(`${staffUrl('a')}/console/clips?kind=tap`);
  await expect(a.locator('tbody tr')).toHaveCount(3);
  for (const box of ['box-a', 'box-b', 'box-c']) await expect(a.locator(`tbody tr[data-box="${box}"]`)).toHaveCount(1);

  // Keep Box B's clip from Box A; Box B shows it kept and its log names who.
  await a.locator('tr[data-box="box-b"]').getByRole('button', { name: /^Keep / }).click();
  await expect(a.getByRole('status')).toHaveText('Kept for 1 year.');
  const b = await staffPage(browser, 'b');
  await b.goto(`${staffUrl('b')}/console/clips?kept=1`);
  await expect(b.locator('tbody tr')).toHaveCount(1);

  // Download Box C's clip through Box A: the same bytes as on Box C.
  await a.goto(`${staffUrl('a')}/console/clips?kind=tap`);
  const [download] = await Promise.all([a.waitForEvent('download'), a.locator('tr[data-box="box-c"]').getByRole('link', { name: /^Download Court 3/ }).click()]);
  expect(download.suggestedFilename()).toMatch(/^court-3-\d{8}-\d{6}-wide\.mp4$/);
  const size = Number(vc('exec', '-T', 'venue-c', 'sh', '-c', `stat -c %s /data/clips/${tokens['Court 3']}/wide.mp4`).trim());
  expect(fs.statSync(await download.path()).size).toBe(size);

  // Delete Box B's clip from Box A, with the confirmation naming it.
  await a.locator('tr[data-box="box-b"]').getByRole('link', { name: /^Delete / }).click();
  await expect(a.getByText(/Court 2 · .* It can't be recovered\./)).toBeVisible();
  await a.getByRole('button', { name: 'Delete' }).click();
  await expect(a.getByRole('status')).toHaveText('Clip deleted.');
  // Every console shows the whole venue: Box B's own clip is gone, the others remain.
  await b.goto(`${staffUrl('b')}/console/clips?kind=tap`);
  await expect(b.locator('tbody tr[data-box="box-b"]')).toHaveCount(0);
  await expect(b.locator('tbody tr')).toHaveCount(2);
  await b.goto(`${staffUrl('b')}/console/log`);
  await expect(b.locator('tbody tr').filter({ hasText: 'deleted' }).first()).toContainText('olive (via another box)');
});

test('an unreachable box shows as such while the rest of the venue works (FR-41)', async ({ browser }) => {
  vc('stop', 'venue-c');
  const a = await staffPage(browser, 'a');
  await courtStatusOn(a, 'a', 'Court 3', /^Box unreachable since \d{1,2}:\d{2} [ap]m$/, 90_000);
  await expect(a.locator('tr[data-box="box-c"] [data-unreachable]')).toBeVisible();
  await courtStatusOn(a, 'a', 'Court 2', /^Recording since/);
  await a.goto(`${staffUrl('a')}/console/clips`);
  await expect(a.getByText('Not shown: clips on box-c')).toBeVisible();
});

test('a replacement box joins the venue and gets its courts, logo and staff from its siblings (FR-43)', async ({ browser }) => {
  test.setTimeout(240_000);
  // Box C's disk is replaced: same name and venue key, empty data.
  vc('rm', '-fs', 'venue-c');
  execFileSync('docker', ['volume', 'rm', '-f', 'ch-test_venue-c-data'], { stdio: 'ignore' });
  vc('up', '-d', 'venue-c');
  await waitUp('c');
  const c = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await expect.poll(async () => {
    await signInOn(c, 'c');
    return c.url();
  }, { timeout: 60_000, intervals: [3000] }).toBe(`${staffUrl('c')}/console`);
  await courtStatusOn(c, 'c', 'Court 3', /^Recording since/, 90_000);
  expect(((await c.locator('tr[data-court="Court 3"] [data-court-code]').textContent()) ?? '').trim(), 'the sign on the net post still works').toBe(codes['Court 3']);
  const logo = await c.request.get(`${staffUrl('c')}/console/logo`);
  expect(Buffer.from(await logo.body()).equals(fs.readFileSync(LOGO))).toBe(true);
});

test('a device without the venue key gets nothing from the sync interface (FR-42)', async ({ request }) => {
  expect((await request.get('http://localhost:18446/sync/state')).status()).toBe(401);
  const forged = await request.get('http://localhost:18446/sync/state', { headers: { 'x-venue-auth': `${Date.now()}.${'ab'.repeat(32)}` } });
  expect(forged.status()).toBe(401);
  expect((await request.get('http://localhost:18446/sync/clips?limit=50')).status()).toBe(401);
});

test('all boxes keep the time the Owner set on one of them (FR-38)', async ({ browser }) => {
  const a = await staffPage(browser, 'a');
  await a.goto(`${staffUrl('a')}/console/settings`);
  const wanted = new Date(Date.now() + 8 * 3600_000 + 3 * 60_000);
  await a.getByLabel('Correct time').fill(wanted.toISOString().slice(0, 16));
  await a.getByRole('button', { name: 'Correct the time' }).click();
  await expect(a.getByRole('status')).toHaveText('Box time corrected.');
  const hhmm = (d: Date): string => `${((d.getUTCHours() + 11) % 12) + 1}:${String(d.getUTCMinutes()).padStart(2, '0')}`;
  const b = await staffPage(browser, 'b');
  await expect.poll(async () => {
    await b.goto(`${staffUrl('b')}/console/settings`);
    return b.locator('[data-box-time]').textContent();
  }, { timeout: 60_000, intervals: [3000] }).toContain(hhmm(wanted));
});

test('a one-box venue restores a replacement box from the settings backup (FR-43, FR-44)', async ({ browser, page }) => {
  test.setTimeout(240_000);
  const a = await staffPage(browser, 'a');
  await a.goto(`${staffUrl('a')}/console/settings`);
  await a.getByLabel('Backup passphrase').fill('court backup passphrase');
  const [download] = await Promise.all([a.waitForEvent('download'), a.getByRole('button', { name: 'Export settings backup' }).click()]);
  expect(download.suggestedFilename()).toMatch(/^court-highlights-settings-\d{8}\.chbackup$/);
  const backup = await download.path();
  const text = fs.readFileSync(backup, 'utf8');
  expect(text, 'no plaintext passwords or camera credentials').not.toMatch(/argon2|rtsp:|password_hash/);
  vc('down', '-v', '--remove-orphans');

  await freshBox();
  await startCameras('cam-3');
  await page.goto(`${STAFF}/setup`);
  await page.getByLabel('Settings backup file').setInputFiles(backup);
  await page.getByLabel('Backup passphrase').fill('the wrong passphrase');
  await page.getByRole('button', { name: 'Restore from backup' }).click();
  await expect(page.getByText("That file isn't a settings backup, or the passphrase is wrong.")).toBeVisible();
  await page.goto(`${STAFF}/setup`);
  await page.getByLabel('Settings backup file').setInputFiles(backup);
  await page.getByLabel('Backup passphrase').fill('court backup passphrase');
  await page.getByRole('button', { name: 'Restore from backup' }).click();
  await expect(page.getByRole('status')).toHaveText('Settings restored. Sign in with your usual account.');
  await page.getByLabel('Username').fill(OWNER.username);
  await page.getByLabel('Password').fill(OWNER.password);
  await page.getByRole('button', { name: 'Sign in' }).click();
  await expect(page).toHaveURL(`${STAFF}/console`);
  // The venue's courts and settings came back; this box (box-a) records the courts that were Box A's.
  for (const court of ['Court 1', 'Court 2', 'Court 3']) await expect(page.locator(`tr[data-court="${court}"]`)).toBeVisible();
  await page.goto(`${STAFF}/console/settings`);
  await expect(page.getByLabel('Seconds before the tap')).toHaveValue('12');
  const player = await browser.newPage();
  await player.goto(`${PLAYER}/c/${codes['Court 1']}`);
  await expect(player.getByRole('heading', { level: 1 })).toHaveText('Court 1');
});

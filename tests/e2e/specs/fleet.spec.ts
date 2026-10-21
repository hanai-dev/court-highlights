import { test, expect, type Page } from '@playwright/test';
import {
  CAMERA, CLOUD, OWNER, PLAYER, STAFF, addCourt, boxExec, clearMail, cloudSql, cloudUp, compose, completeSetup, courtCode, createAdmin, fleetSignIn, freshBox,
  registerBox, sentMail, signIn, startCameras, totp, waitCourtStatus, type FleetAccount,
} from './helpers';

test.describe.configure({ mode: 'serial' });

let admin: FleetAccount;
const ADMIN_PASSWORD = 'fleet-admin-pass-2026';
let installer: FleetAccount;
const INSTALLER_PASSWORD = 'fleet-installer-pass-26';
let venueId = '';
let code = '';
let spareKey = '';
let boxKeyA = '';

const advanceCloud = async (page: Page, ms: number): Promise<void> => {
  await page.request.post(`${CLOUD}/__test/clock`, { form: { ms: String(ms) } });
};

async function asAdmin(page: Page): Promise<void> {
  await fleetSignIn(page, admin, ADMIN_PASSWORD);
  await expect(page).toHaveURL(`${CLOUD}/fleet`);
}

async function boxSettingsStatus(page: Page): Promise<string> {
  await page.goto(`${STAFF}/console/settings`);
  return (await page.locator('[data-cloud-status]').textContent()) ?? '';
}

test('fleet sign-in needs the password and the authenticator code; temporary passwords are replaced; sessions time out (FR-47)', async ({ page }) => {
  await freshBox();
  await cloudUp();
  admin = createAdmin('root');
  expect(admin.secret).toMatch(/^[A-Z2-7]{32}$/);
  const noCode = await page.request.post(`${CLOUD}/fleet/signin`, { form: { username: 'root', password: admin.password, code: '' }, maxRedirects: 0 });
  expect(noCode.status(), 'password without code is refused').toBe(401);
  await fleetSignIn(page, admin, admin.password, '000000');
  await expect(page.getByRole('alert')).toHaveText("That username, password or code isn't right.");

  await fleetSignIn(page, admin);
  await expect(page).toHaveURL(`${CLOUD}/fleet/account`);
  await page.goto(`${CLOUD}/fleet`);
  await expect(page).toHaveURL(`${CLOUD}/fleet/account`);
  await page.getByLabel('Temporary password').fill(admin.password);
  await page.getByLabel('New password').fill(ADMIN_PASSWORD);
  await page.getByRole('button', { name: 'Change password' }).click();
  await expect(page).toHaveURL(`${CLOUD}/fleet`);
  const cookie = (await page.context().cookies()).find((c) => c.name === '__Host-ch_fleet')!;
  expect([cookie.secure, cookie.httpOnly, cookie.sameSite]).toEqual([true, true, 'Strict']);

  // 30 minutes idle ends the session.
  await advanceCloud(page, 31 * 60_000);
  await page.goto(`${CLOUD}/fleet`);
  await expect(page).toHaveURL(`${CLOUD}/fleet/signin`);
  await asAdmin(page);
});

test('an Admin adds a venue and registers a box; its key is shown once; the box checks in with its health (FR-48, FR-49)', async ({ page, browser }) => {
  await asAdmin(page);
  await page.goto(`${CLOUD}/fleet/venues`);
  await page.getByLabel('Venue name').fill(OWNER.venue);
  await page.getByRole('button', { name: 'Add venue' }).click();
  venueId = page.url().split('/').pop()!;
  await page.getByLabel('Box name').fill('Box A');
  await page.getByRole('button', { name: 'Register box' }).click();
  const key = (await page.locator('[data-box-key]').textContent())!.trim();
  expect(key).toMatch(/^[0-9a-f]{64}$/);
  boxKeyA = key;
  await page.goto(`${CLOUD}/fleet/venues/${venueId}`);
  await expect(page.locator('[data-box-key]')).toHaveCount(0);

  const staff = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await startCameras('cam-3');
  await completeSetup(staff);
  await addCourt(staff, 'Court 1', CAMERA(3));
  code = await courtCode(staff, 'Court 1');
  await waitCourtStatus(staff, 'Court 1', /^Recording since/);
  await staff.goto(`${STAFF}/console/settings`);
  await staff.getByLabel('Box key').fill(key);
  await staff.getByRole('button', { name: 'Save box key' }).click();
  await expect.poll(() => boxSettingsStatus(staff), { timeout: 30_000 }).toMatch(/^Connected\./);

  await page.goto(`${CLOUD}/fleet`);
  const row = page.locator('tr[data-box="Box A"]');
  await expect(row).toContainText('0.1.0');
  await expect(row).toContainText(/\d+ %/);
  await expect(row).toContainText('1 of 1');
  await expect(row.locator('[data-missed]')).toHaveCount(0);
});

test('a box silent for 5 minutes is flagged "missed check-in" (FR-49)', async ({ page }) => {
  await asAdmin(page);
  compose('stop', 'box-a');
  await advanceCloud(page, 6 * 60_000);
  await page.goto(`${CLOUD}/fleet`);
  await expect(page.locator('tr[data-box="Box A"] [data-missed]')).toContainText('Missed check-in');
  compose('start', 'box-a');
  await expect.poll(async () => {
    await page.goto(`${CLOUD}/fleet`);
    return page.locator('tr[data-box="Box A"] [data-missed]').count();
  }, { timeout: 60_000, intervals: [3000] }).toBe(0);
});

test('switched off with a reason, court pages hide the email box and nothing is sent; switched on, the waiting videos go (FR-27, FR-50, GV-26)', async ({ page, browser }) => {
  test.setTimeout(300_000);
  await clearMail();
  const p = await (await browser.newContext()).newPage();
  await p.goto(`${PLAYER}/c/${code}`);
  await p.getByRole('button', { name: 'Start game' }).click();
  await p.getByLabel('Email address').fill('zed@example.com');
  await p.getByRole('button', { name: 'Add' }).click();

  await asAdmin(page);
  await page.goto(`${CLOUD}/fleet/venues/${venueId}`);
  await page.getByLabel('Reason').fill('invoice late');
  await page.getByRole('button', { name: 'Switch online features off' }).click();
  await expect(page.locator('[data-online]')).toHaveText('Off');
  await expect.poll(async () => {
    await p.goto(`${PLAYER}/c/${code}`);
    return p.getByText("Download your videos here — email isn't available at this venue.").count();
  }, { timeout: 30_000 }).toBe(1);
  await expect(p.getByLabel('Email address')).toHaveCount(0);
  await p.getByRole('button', { name: 'Stop game' }).click();
  await expect(p.locator('[data-game-video]')).toBeVisible({ timeout: 120_000 });
  await p.waitForTimeout(15_000);
  expect(await sentMail(), 'nothing is sent while switched off').toHaveLength(0);

  await page.getByLabel('Reason').fill('invoice paid');
  await page.getByRole('button', { name: 'Switch online features on' }).click();
  await expect.poll(async () => (await sentMail()).map((m) => m.to[0]), { timeout: 120_000, intervals: [3000] }).toEqual(['zed@example.com']);
  await page.goto(`${CLOUD}/fleet/log`);
  await expect(page.locator('tbody tr').filter({ hasText: 'switched online features off' })).toContainText('invoice late');
  await expect(page.locator('tbody tr').filter({ hasText: 'switched online features off' })).toContainText('root');
  // The box's own activity log shows both switches, made by the fleet (F-LOG).
  const staff = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await signIn(staff, OWNER.username, OWNER.password);
  await staff.goto(`${STAFF}/console/log`);
  for (const action of ['online features switched off', 'online features switched on']) {
    await expect(staff.locator('tbody tr').filter({ hasText: action }).first()).toContainText('fleet');
  }
});

test('an Installer registers boxes but cannot switch, revoke or manage users; 5 wrong tries lock an account (FR-47, FR-48)', async ({ page, browser }) => {
  await asAdmin(page);
  await page.goto(`${CLOUD}/fleet/users`);
  await page.getByLabel('Username').fill('inst');
  await page.getByLabel('Role').selectOption('installer');
  await page.getByRole('button', { name: 'Add user' }).click();
  installer = {
    username: 'inst',
    password: (await page.locator('[data-temp-password]').textContent())!.trim(),
    secret: (await page.locator('[data-totp-secret]').textContent())!.trim(),
  };
  const i = await (await browser.newContext()).newPage();
  await fleetSignIn(i, installer);
  await i.getByLabel('Temporary password').fill(installer.password);
  await i.getByLabel('New password').fill(INSTALLER_PASSWORD);
  await i.getByRole('button', { name: 'Change password' }).click();
  await expect(i).toHaveURL(`${CLOUD}/fleet`);
  await expect(i.getByRole('link', { name: 'Users' })).toHaveCount(0);
  await expect(i.getByRole('link', { name: 'Fleet log' })).toHaveCount(0);
  for (const [method, url] of [['GET', '/fleet/users'], ['GET', '/fleet/log'], ['POST', `/fleet/venues/${venueId}/switch`], ['POST', '/fleet/boxes/1/revoke']] as const) {
    const res = await i.request.fetch(`${CLOUD}${url}`, { method, form: method === 'POST' ? { on: '0', reason: 'test' } : undefined, headers: { Origin: CLOUD } });
    expect(res.status(), `${method} ${url}`).toBe(403);
  }
  await i.goto(`${CLOUD}/fleet/venues/${venueId}`);
  await expect(i.getByRole('button', { name: /Switch online features/ })).toHaveCount(0);
  await expect(i.getByRole('button', { name: /^Revoke/ })).toHaveCount(0);
  await i.getByLabel('Box name').fill('Box B');
  await i.getByRole('button', { name: 'Register box' }).click();
  spareKey = (await i.locator('[data-box-key]').textContent())!.trim();
  expect(spareKey).toMatch(/^[0-9a-f]{64}$/);

  // Lockout: 5 wrong passwords, then even the right password and code are refused for 15 minutes.
  const l = await (await browser.newContext()).newPage();
  for (let n = 0; n < 5; n++) await fleetSignIn(l, installer, 'wrong-password-123', totp(installer.secret));
  await fleetSignIn(l, installer, INSTALLER_PASSWORD);
  await expect(l.getByRole('alert')).toHaveText("That username, password or code isn't right.");
  await page.goto(`${CLOUD}/fleet/log`);
  await expect(page.locator('tbody tr').filter({ hasText: 'locked for 15 minutes' }).first()).toContainText('inst');
  await advanceCloud(page, 16 * 60_000);
  await fleetSignIn(l, installer, INSTALLER_PASSWORD);
  await expect(l).toHaveURL(`${CLOUD}/fleet`);
});

test('a revoked key is refused: the box can no longer report or send, and nothing is stored (FR-48)', async ({ page, browser }) => {
  test.setTimeout(200_000);
  await asAdmin(page);
  const itemsBefore = cloudSql('SELECT count(*) FROM item');
  await page.goto(`${CLOUD}/fleet/venues/${venueId}`);
  await page.getByRole('button', { name: 'Revoke the key of Box A' }).click();
  await expect(page.locator('tr[data-box="Box A"]')).toContainText('Key revoked');
  const staff = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await signIn(staff, OWNER.username, OWNER.password);
  await expect.poll(() => boxSettingsStatus(staff), { timeout: 30_000 }).toBe('The online service refused this key. Ask your installer for a new one.');

  await clearMail();
  const p = await (await browser.newContext()).newPage();
  await p.goto(`${PLAYER}/c/${code}`);
  await p.getByRole('button', { name: 'Start game' }).click();
  // The box can't learn anything new, so it still offers email and keeps the address on the box.
  await p.getByLabel('Email address').fill('yan@example.com');
  await p.getByRole('button', { name: 'Add' }).click();
  await p.getByRole('button', { name: 'Stop game' }).click();
  await expect(p.locator('[data-game-video]')).toBeVisible({ timeout: 120_000 });
  await p.waitForTimeout(15_000);
  expect(await sentMail()).toHaveLength(0);
  expect(cloudSql('SELECT count(*) FROM item'), 'nothing stored from a revoked box').toBe(itemsBefore);
  const refused = await page.request.post(`${CLOUD}/api/v1/checkin`, {
    headers: { Authorization: `Bearer ${boxKeyA}` },
    data: { version: 'x', disk_pct: 1, courts_recording: 0, courts_total: 0 },
  });
  expect(refused.status()).toBe(401);
});

test("one venue's key cannot reach another venue's items, cross-site fleet writes change nothing, and no fleet route serves footage (SEC-CROSS-VENUE, FR-53)", async ({ page }) => {
  const otherKey = registerBox('Riverside Club');
  const api = (key: string) => ({ Authorization: `Bearer ${key}` });
  const made = new Date().toISOString();
  const expires = new Date(Date.now() + 3 * 86400_000).toISOString();
  const created = await page.request.post(`${CLOUD}/api/v1/items`, {
    headers: api(otherKey),
    data: { ref: 'item-1', kind: 'tap', court: 'River 1', made_at: made, expires_at: expires },
  });
  expect(created.status()).toBe(200);
  const { id, upload_url } = (await created.json()) as { id: number; upload_url: string };
  // Upload URLs are signed for the store's address on the box network, so upload from there like a box does.
  boxExec(`printf 'ftypmp42-test' > /tmp/item.mp4 && curl -fsS -X PUT -H 'content-type: video/mp4' --data-binary @/tmp/item.mp4 '${upload_url}'`);
  expect((await page.request.post(`${CLOUD}/api/v1/items/${id}/done`, { headers: api(otherKey) })).status()).toBe(204);

  // Venue A's (unrevoked) key: Riverside's item is invisible.
  expect((await page.request.post(`${CLOUD}/api/v1/items/${id}/done`, { headers: api(spareKey) })).status()).toBe(404);
  expect((await page.request.delete(`${CLOUD}/api/v1/items/${id}`, { headers: api(spareKey) })).status()).toBe(404);
  const steal = await page.request.post(`${CLOUD}/api/v1/deliveries`, { headers: api(spareKey), data: { ref: 'steal', title: 't', items: [id], recipients: ['x@example.com'] } });
  expect(steal.status(), 'cannot email another venue\'s item').not.toBe(201);
  expect(cloudSql(`SELECT count(*) FROM delivery_item WHERE item_id = ${id}`)).toBe('0');

  // Fleet sessions: no clip listing or download anywhere; cross-site writes refused with no effect.
  await asAdmin(page);
  for (const url of ['/fleet', '/fleet/venues', `/fleet/venues/${venueId}`, '/fleet/log', '/fleet/users']) {
    await page.goto(`${CLOUD}${url}`);
    expect(await page.content(), url).not.toMatch(/\/v\/[0-9a-f]{64}|\.mp4/);
  }
  expect((await page.request.get(`${CLOUD}/fleet/items`)).status()).toBe(404);
  expect((await page.request.delete(`${CLOUD}/api/v1/items/${id}`)).status(), 'a fleet cookie is not a box key').toBe(401);
  const csrf = await page.request.post(`${CLOUD}/fleet/venues/${venueId}/switch`, { form: { on: '0', reason: 'csrf' }, headers: { Origin: 'http://evil.example' } });
  expect(csrf.status()).toBe(403);
  await page.goto(`${CLOUD}/fleet/venues/${venueId}`);
  await expect(page.locator('[data-online]')).toHaveText('On');
  await page.goto(`${CLOUD}/fleet/log`);
  for (const action of ['signed in', 'added venue', 'registered box', 'revoked box key', 'switched online features on', 'added fleet user']) {
    await expect(page.locator('tbody tr').filter({ hasText: action }).first()).toBeVisible();
  }
});

test('Wipe box: the Owner types the venue name; clips, footage, accounts and logs are gone from the disk and the fleet no longer lists the box (FR-55, SEC-WIPE)', async ({ page, browser }) => {
  test.setTimeout(240_000);
  const staff = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await signIn(staff, OWNER.username, OWNER.password);
  await staff.goto(`${STAFF}/console/settings`);
  await staff.getByLabel('Box key').fill(spareKey);
  await staff.getByRole('button', { name: 'Save box key' }).click();
  await expect.poll(() => boxSettingsStatus(staff), { timeout: 30_000 }).toMatch(/^Connected\./);
  const p = await (await browser.newContext()).newPage();
  await p.goto(`${PLAYER}/c/${code}`);
  await p.getByRole('button', { name: 'Clip that' }).click();
  await expect(p.locator('[data-clip-state="ready"]').first()).toBeVisible({ timeout: 60_000 });
  expect(Number(boxExec('find /data -name "*.mp4" | wc -l').trim())).toBeGreaterThan(0);

  await staff.goto(`${STAFF}/console/settings`);
  await staff.getByRole('link', { name: 'Wipe this box…' }).click();
  await staff.getByLabel(/Type the venue name to confirm/).fill('Some other venue');
  await staff.getByRole('button', { name: 'Wipe this box' }).click();
  await expect(staff.locator('#confirm-error')).toContainText(`Type the venue name exactly as shown: ${OWNER.venue}`);
  expect(Number(boxExec('find /data -name "*.mp4" | wc -l').trim()), 'a wrong name erases nothing').toBeGreaterThan(0);

  await staff.getByLabel(/Type the venue name to confirm/).fill(OWNER.venue);
  await staff.getByRole('button', { name: 'Wipe this box' }).click();
  await expect(staff.getByRole('heading', { name: 'This box is wiped' })).toBeVisible();
  // The box process ends; its service manager (systemd Restart=always on the box) starts it again.
  await expect.poll(() => compose('ps', '-a', '--format', '{{.State}}', 'box-a').trim(), { timeout: 30_000 }).toBe('exited');
  compose('start', 'box-a');
  await expect.poll(async () => (await fetch(`${PLAYER}/healthz`).catch(() => null))?.ok ?? false, { timeout: 60_000 }).toBe(true);

  await staff.goto(`${STAFF}/console`);
  await expect(staff).toHaveURL(`${STAFF}/setup`);
  const files = boxExec('find /data -type f').trim().split('\n');
  expect(files.filter((f) => /\.(mp4|jpg|png|ts|mkv)$/.test(f)), 'no footage, clips or logos').toEqual([]);
  for (const text of [OWNER.venue, OWNER.username, 'argon2id', code]) {
    expect(boxExec(`grep -r -a -l "${text}" /data || true`).trim(), `nothing on the disk mentions ${text}`).toBe('');
  }
  await asAdmin(page);
  await page.goto(`${CLOUD}/fleet/venues/${venueId}`);
  await expect(page.locator('tr[data-box="Box B"]')).toHaveCount(0);
  await page.goto(`${CLOUD}/fleet/log`);
  await expect(page.locator('tbody tr').filter({ hasText: 'wiped box' }).first()).toContainText('Box B');
  const checkin = await page.request.post(`${CLOUD}/api/v1/checkin`, {
    headers: { Authorization: `Bearer ${spareKey}` },
    data: { version: 'x', disk_pct: 1, courts_recording: 0, courts_total: 0 },
  });
  expect(checkin.status(), 'the wiped box key no longer works').toBe(401);
});

test('an Admin deactivates a fleet user who left: their open session ends and they cannot sign in again (security operations)', async ({ page, browser }) => {
  await asAdmin(page);
  await page.goto(`${CLOUD}/fleet/users`);
  await expect(page.getByRole('button', { name: 'Deactivate root' }), 'no button for your own account').toHaveCount(0);
  await page.getByLabel('Username').fill('leaver');
  await page.getByRole('button', { name: 'Add user' }).click();
  const leaver: FleetAccount = {
    username: 'leaver',
    password: (await page.locator('[data-temp-password]').textContent())!.trim(),
    secret: (await page.locator('[data-totp-secret]').textContent())!.trim(),
  };
  const l = await (await browser.newContext()).newPage();
  await fleetSignIn(l, leaver);
  await l.getByLabel('Temporary password').fill(leaver.password);
  await l.getByLabel('New password').fill('leaver-own-password-26');
  await l.getByRole('button', { name: 'Change password' }).click();
  await expect(l).toHaveURL(`${CLOUD}/fleet`);

  await page.goto(`${CLOUD}/fleet/users`);
  await page.getByRole('button', { name: 'Deactivate leaver' }).click();
  await expect(page.locator('tbody tr').filter({ hasText: 'leaver' })).toContainText('Deactivated');
  await l.goto(`${CLOUD}/fleet`);
  await expect(l).toHaveURL(`${CLOUD}/fleet/signin`);
  await fleetSignIn(l, leaver, 'leaver-own-password-26');
  await expect(l.getByRole('alert')).toHaveText("That username, password or code isn't right.");
  await page.goto(`${CLOUD}/fleet/log`);
  await expect(page.locator('tbody tr').filter({ hasText: 'deactivated fleet user' }).first()).toContainText('leaver');
});

// Last in this file: it moves the service's clock 91 days on.
test('health records older than 90 days are removed (FR-49)', async ({ page }) => {
  test.setTimeout(180_000);
  const key = compose('exec', '-T', 'cloud', './target/debug/ch-cloud', 'add-box', OWNER.venue, 'box-r').trim();
  const checkin = await page.request.post(`${CLOUD}/api/v1/checkin`, {
    headers: { Authorization: `Bearer ${key}` },
    data: { version: '0.1.0', disk_pct: 3, courts_recording: 0, courts_total: 0 },
  });
  expect(checkin.ok()).toBe(true);
  const newest = cloudSql('SELECT max(at) FROM checkin');
  expect(Number(cloudSql(`SELECT count(*) FROM checkin WHERE at <= '${newest}'`))).toBeGreaterThan(0);
  await advanceCloud(page, 91 * 86_400_000);
  // The clean-up runs every minute.
  await expect.poll(() => cloudSql(`SELECT count(*) FROM checkin WHERE at <= '${newest}'`), { timeout: 100_000, intervals: [5000] }).toBe('0');
});

import { test, expect, type Browser } from '@playwright/test';
import { OWNER, PLAYER, STAFF, advanceClock, boxExec, completeSetup, freshBox, signIn } from './helpers';

test.describe.configure({ mode: 'serial' });

async function addStaff(browser: Browser, page: import('@playwright/test').Page, name: string, username: string): Promise<string> {
  await page.goto(`${STAFF}/console/staff/new`);
  await page.getByLabel('Name', { exact: true }).fill(name);
  await page.getByLabel('Username', { exact: true }).fill(username);
  await page.getByRole('button', { name: /Add staff/ }).click();
  const temp = (await page.locator('[data-temp-password]').textContent())?.trim() ?? '';
  expect(temp).toHaveLength(16);
  return temp;
}

test('fresh box: first start opens setup and no built-in account works (FR-40, v5.0.0-6.3.2)', async ({ page }) => {
  await freshBox();
  await page.goto(`${STAFF}/console`);
  await expect(page).toHaveURL(`${STAFF}/setup`);
  await expect(page.getByRole('img', { name: /camera mounted high/ })).toBeVisible();
  for (const [u, p] of [['admin', 'admin'], ['root', 'root'], ['owner', 'password'], ['admin', 'admin123456789']]) {
    const res = await page.request.post(`${STAFF}/console/signin`, { form: { username: u, password: p }, maxRedirects: 0 });
    expect(res.status(), `${u}/${p}`).toBe(401);
  }
});

test('setup refuses weak passwords, then creates the owner with an argon2 hash (v5.0.0-6.2.1, 6.2.4, 11.4.2)', async ({ page }) => {
  for (const bad of ['short-pass', '1qaz2wsx3edc', 'Q1W2E3R4T5Y6']) {
    await page.goto(`${STAFF}/setup`);
    await page.getByLabel('Venue name').fill(OWNER.venue);
    await page.getByLabel('Your name').fill(OWNER.name);
    await page.getByLabel('Owner username').fill(OWNER.username);
    await page.getByLabel('Owner password').fill(bad);
    await page.locator('#password').evaluate((el) => el.removeAttribute('minlength'));
    await page.getByRole('button', { name: 'Continue' }).click();
    await expect(page.getByRole('alert')).toContainText(/at least 12 characters|too common/);
  }
  await completeSetup(page);
  await page.goto(`${STAFF}/console`);
  await expect(page.getByRole('heading', { name: 'No courts yet' })).toBeVisible();
  // WAL mode: fresh rows may still live in box.db-wal, so search both files.
  expect(Number(boxExec('cat /data/box.db /data/box.db-wal 2>/dev/null | grep -a -c argon2id').trim())).toBeGreaterThan(0);
  expect(boxExec(`cat /data/box.db /data/box.db-wal 2>/dev/null | grep -a -c "${OWNER.password}" || true`).trim()).toBe('0');
});

test('staff lifecycle: temporary password, forced change, reset, deactivate ends access (FR-29, v5.0.0-6.4.1, 7.4.2)', async ({ browser, page }) => {
  await signIn(page, OWNER.username, OWNER.password);
  const temp = await addStaff(browser, page, 'Ken Staff', 'ken');

  const ken = await browser.newContext({ ignoreHTTPSErrors: true });
  const kenPage = await ken.newPage();
  await signIn(kenPage, 'ken', temp);
  await expect(kenPage).toHaveURL(`${STAFF}/console/account?first=1`);
  await kenPage.goto(`${STAFF}/console/clips`); // any page sends him back until he changes it
  await kenPage.goto(`${STAFF}/console`);
  await expect(kenPage).toHaveURL(/\/console\/account\?first=1/);
  await kenPage.getByLabel('Current password').fill(temp);
  await kenPage.getByLabel('New password').fill('ken-own-password-2026');
  await kenPage.getByRole('button', { name: 'Change password' }).click();
  await expect(kenPage.getByRole('status')).toContainText('Password changed');
  await kenPage.goto(`${STAFF}/console`);
  await expect(kenPage.getByRole('heading', { name: 'Courts today' })).toBeVisible();

  // Owner resets Ken's password: Ken's open sign-in stops working at once.
  await page.goto(`${STAFF}/console/staff`);
  await page.getByRole('link', { name: 'Edit Ken Staff' }).click();
  await page.getByRole('button', { name: 'Reset password' }).click();
  const temp2 = (await page.locator('[data-temp-password]').textContent())?.trim() ?? '';
  expect(temp2).not.toBe(temp);
  await kenPage.goto(`${STAFF}/console`);
  await expect(kenPage).toHaveURL(`${STAFF}/console/signin`);
  await signIn(kenPage, 'ken', temp2);
  await kenPage.getByLabel('Current password').fill(temp2);
  await kenPage.getByLabel('New password').fill('ken-second-password-26');
  await kenPage.getByRole('button', { name: 'Change password' }).click();
  await expect(kenPage.getByRole('status')).toContainText('Password changed');

  // Deactivation ends the open sign-in on the next request.
  await page.goto(`${STAFF}/console/staff`);
  await page.getByRole('link', { name: 'Edit Ken Staff' }).click();
  await page.getByRole('button', { name: 'Deactivate Ken Staff' }).click();
  await expect(page.getByRole('status')).toContainText('Deactivated');
  await kenPage.goto(`${STAFF}/console`);
  await expect(kenPage).toHaveURL(`${STAFF}/console/signin`);
  await signIn(kenPage, 'ken', 'ken-second-password-26');
  await expect(kenPage.getByRole('alert')).toContainText('Wrong username or password');
  await ken.close();

  // Staff changes are in the activity log, with who made them (F-LOG).
  await page.goto(`${STAFF}/console/log`);
  for (const action of ['added staff', 'reset password', 'deactivated staff']) {
    await expect(page.locator('tbody tr').filter({ hasText: action }).first()).toContainText(OWNER.username);
  }
});

test('a temporary password expires after 72 h unused (v5.0.0-6.4.1)', async ({ browser, page, request }) => {
  await signIn(page, OWNER.username, OWNER.password);
  const temp = await addStaff(browser, page, 'Ana Late', 'ana');
  await advanceClock(request, 72 * 3600 * 1000 + 1000);
  const ana = await browser.newContext({ ignoreHTTPSErrors: true });
  const anaPage = await ana.newPage();
  await signIn(anaPage, 'ana', temp);
  await expect(anaPage.getByRole('alert')).toContainText('Wrong username or password');
  await ana.close();
});

test('5 consecutive wrong passwords lock the account for 15 minutes (FR-28, GV-16)', async ({ page, request }) => {
  await freshBox();
  await completeSetup(page);
  const tryPassword = (password: string) =>
    request.post(`${STAFF}/console/signin`, { form: { username: OWNER.username, password }, maxRedirects: 0 });
  for (let i = 0; i < 5; i++) expect((await tryPassword('wrong-password-xyz')).status()).toBe(401);
  expect((await tryPassword(OWNER.password)).status(), 'right password refused while locked').toBe(401);
  await advanceClock(request, 15 * 60 * 1000);
  expect((await tryPassword(OWNER.password)).status(), 'accepted after 15 minutes').toBe(303);
});

test('staff pages exist only on the TLS listener; the player listener has none (A-124)', async ({ request }) => {
  for (const p of ['/console', '/console/signin', '/setup', '/console/staff']) {
    expect((await request.get(`${PLAYER}${p}`, { maxRedirects: 0 })).status(), p).toBe(404);
  }
  const tls = await request.get(`${STAFF}/console/signin`);
  expect(tls.ok()).toBeTruthy();
  expect(tls.headers()['content-security-policy']).toContain("frame-ancestors 'none'");
  expect(tls.headers()['strict-transport-security']).toBeUndefined();
});

test('staff cannot reach owner pages; cross-site writes are refused (v5.0.0-8.2.1, 3.5.1)', async ({ browser, page }) => {
  await signIn(page, OWNER.username, OWNER.password);
  const temp = await addStaff(browser, page, 'Sam Staff', 'sam');
  const sam = await browser.newContext({ ignoreHTTPSErrors: true });
  const samPage = await sam.newPage();
  await signIn(samPage, 'sam', temp);
  await samPage.getByLabel('Current password').fill(temp);
  await samPage.getByLabel('New password').fill('sam-own-password-2026');
  await samPage.getByRole('button', { name: 'Change password' }).click();
  await expect(samPage.getByRole('link', { name: 'Staff' })).toHaveCount(0);
  for (const p of ['/console/staff', '/console/staff/new', '/console/staff/1']) {
    const res = await samPage.goto(`${STAFF}${p}`);
    expect(res?.status(), p).toBe(403);
  }
  const forged = await samPage.request.post(`${STAFF}/console/staff`, {
    form: { display_name: 'Mallory', username: 'mallory', role: 'owner' },
    headers: { origin: 'https://evil.example' },
  });
  expect(forged.status()).toBe(403);
  const asStaff = await samPage.request.post(`${STAFF}/console/staff`, { form: { display_name: 'Mallory', username: 'mallory', role: 'owner' } });
  expect(asStaff.status()).toBe(403);
  await page.goto(`${STAFF}/console/staff`);
  await expect(page.getByText('Mallory'), 'no account was made').toHaveCount(0);
  // The session cookie is host-only, Secure, HttpOnly and SameSite=Strict; no route opens itself to other origins.
  const cookie = (await sam.cookies()).find((c) => c.name === '__Host-ch_staff')!;
  expect([cookie.secure, cookie.httpOnly, cookie.sameSite]).toEqual([true, true, 'Strict']);
  for (const url of [`${STAFF}/console`, `${STAFF}/console/clips`, `${PLAYER}/`]) {
    const res = await samPage.request.get(url, { headers: { Origin: 'https://evil.example' } });
    expect(res.headers()['access-control-allow-origin'], url).toBeUndefined();
  }
  await sam.close();
});

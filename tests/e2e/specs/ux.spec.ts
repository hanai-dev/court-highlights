import { test, expect, type Browser, type Locator, type Page } from '@playwright/test';
import AxeBuilder from '@axe-core/playwright';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CAMERA, CLOUD, OWNER, PLAYER, STAFF, addCourt, clearMail, cloudSql, cloudUp, completeSetup, connectBox, courtCode, createAdmin, fleetSignIn,
  freshBox, sentMail, signIn, startCameras, waitCourtStatus,
} from './helpers';

test.describe.configure({ mode: 'serial' });

const LOGO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../fixtures/logo-magenta.png');
const ADMIN_PASSWORD = 'ux-fleet-admin-pass-26';
let code = '';
let clipToken = '';
let videosLink = '';
let neverLink = '';
let venueId = '';

async function seriousViolations(page: Page): Promise<string[]> {
  const result = await new AxeBuilder({ page }).withTags(['wcag2a', 'wcag2aa', 'wcag21a', 'wcag21aa', 'wcag22aa']).analyze();
  return result.violations
    .filter((v) => v.impact === 'serious' || v.impact === 'critical')
    .map((v) => `${v.id} (${v.impact}): ${v.nodes.map((n) => n.target.join(' ')).slice(0, 3).join(', ')}`);
}

/** Every route loads with no console or page errors and nothing serious for axe. (The design scan's own console
 * check is skipped: its text-spacing probe injects a style our CSP refuses, by design.) */
async function axeRoutes(page: Page, base: string, routes: string[]): Promise<void> {
  const errors: string[] = [];
  page.on('console', (m) => m.type() === 'error' && errors.push(m.text()));
  page.on('pageerror', (e) => errors.push(e.message));
  for (const route of routes) {
    errors.length = 0;
    const res = await page.goto(`${base}${route}`);
    expect(res?.ok(), `${base}${route} loads`).toBe(true);
    expect(await seriousViolations(page), `${base}${route}`).toEqual([]);
    expect(errors, `${base}${route} console`).toEqual([]);
  }
}

/** Moves focus with Tab only until `target` has it (keyboard reachability in tab order). */
async function tabTo(page: Page, target: Locator, max = 120): Promise<void> {
  for (let i = 0; i < max; i++) {
    if (await target.evaluate((el) => el === document.activeElement).catch(() => false)) return;
    await page.keyboard.press('Tab');
  }
  throw new Error(`not reachable by Tab: ${target}`);
}

/** WCAG contrast ratio between an element's text colour and the first opaque background behind it. */
async function contrast(el: Locator): Promise<number> {
  return el.evaluate((node) => {
    const rgb = (s: string): number[] => (s.match(/[\d.]+/g) ?? []).map(Number);
    const lum = ([r, g, b]: number[]): number => {
      const c = [r, g, b].map((v) => {
        const s = v / 255;
        return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
      });
      return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2];
    };
    let bg: number[] = [255, 255, 255];
    for (let e: Element | null = node; e; e = e.parentElement) {
      const c = rgb(getComputedStyle(e).backgroundColor);
      if (c.length >= 3 && (c.length === 3 || c[3] === 1)) {
        bg = c.slice(0, 3);
        break;
      }
    }
    const [a, b] = [lum(rgb(getComputedStyle(node).color).slice(0, 3)), lum(bg)];
    return (Math.max(a, b) + 0.05) / (Math.min(a, b) + 0.05);
  });
}

async function owner(browser: Browser): Promise<Page> {
  const page = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await signIn(page, OWNER.username, OWNER.password);
  await expect(page).toHaveURL(`${STAFF}/console`);
  return page;
}

test('a venue with clips, a game emailed and a fleet admin, for the checks below', async ({ page, browser }) => {
  test.setTimeout(420_000);
  await freshBox();
  await cloudUp();
  await startCameras('cam-3');
  await completeSetup(page, OWNER, ['00:00', '23:59'], LOGO);
  await connectBox(page);
  await addCourt(page, 'Court 1', CAMERA(3));
  code = await courtCode(page, 'Court 1');
  await waitCourtStatus(page, 'Court 1', /^Recording since/);
  expect((await page.request.post(`${STAFF}/__test/seed-clips`, { form: { court: '1', n: '60' } })).ok()).toBe(true);

  const phone = await (await browser.newContext()).newPage();
  await phone.goto(`${PLAYER}/c/${code}`);
  // The seeded clips are already ready (and have no video): wait for the tapped one, the newest.
  const seededReady = await phone.locator('[data-clip-state="ready"]').count();
  await phone.getByRole('button', { name: 'Clip that' }).click();
  await expect(phone.locator('[data-clip-state="ready"]')).toHaveCount(seededReady + 1, { timeout: 60_000 });
  clipToken = (await phone.locator('[data-clip-state="ready"] a.clip-link').first().getAttribute('href'))!.split('/').pop()!;

  await clearMail();
  await phone.getByRole('button', { name: 'Start game' }).click();
  await phone.getByLabel('Email address').fill('uma@example.com');
  await phone.getByRole('button', { name: 'Add' }).click();
  await phone.waitForTimeout(8_000);
  await phone.getByRole('button', { name: 'Stop game' }).click();
  await expect.poll(async () => (await sentMail()).length, { timeout: 240_000, intervals: [3000] }).toBe(1);
  const [mail] = await sentMail();
  videosLink = mail.text.match(/http:\/\/localhost:18090\/v\/[0-9a-f]{64}(?=\n)/)![0];
  neverLink = mail.text.match(/http:\/\/localhost:18090\/never\/[0-9a-f]{64}/)![0];
  fs.writeFileSync('test-results/email.html', mail.html); // for the design scan

  const admin = createAdmin('uxadmin');
  await fleetSignIn(page, admin);
  await page.getByLabel('Temporary password').fill(admin.password);
  await page.getByLabel('New password').fill(ADMIN_PASSWORD);
  await page.getByRole('button', { name: 'Change password' }).click();
  await expect(page).toHaveURL(`${CLOUD}/fleet`);
  // The Owner's and the fleet admin's sessions together: the fleet pages below and the design scan use them.
  await page.context().storageState({ path: 'test-results/fleet-admin.json' });
  venueId = cloudSql(`SELECT id FROM venue WHERE name = '${OWNER.venue}'`);
});

test('axe finds nothing serious or critical on any route: player, setup, staff console, fleet console, email pages (UX-A11Y)', async ({ browser }) => {
  test.setTimeout(300_000);
  const player = await (await browser.newContext()).newPage();
  await axeRoutes(player, PLAYER, [`/c/${code}`, `/c/${code}/clips/${clipToken}`, '/privacy']);

  const staff = await owner(browser);
  await axeRoutes(staff, STAFF, [
    '/console', '/console/clips', '/console/clips?kind=tap&may_post=1', '/console/footage', '/console/log', '/console/settings', '/console/settings/wipe',
    '/console/staff', '/console/staff/new', '/console/courts', '/console/courts/new', '/console/courts/1', '/console/courts/1/delete', '/console/help',
    '/console/signs', '/console/account', '/setup/courts', '/setup/signs',
  ]);
  const anon = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await axeRoutes(anon, STAFF, ['/console/signin']);

  const fleet = await (await browser.newContext({ storageState: 'test-results/fleet-admin.json' })).newPage();
  await axeRoutes(fleet, CLOUD, ['/fleet', '/fleet/venues', `/fleet/venues/${venueId}`, '/fleet/users', '/fleet/log', '/fleet/account']);
  const visitor = await (await browser.newContext()).newPage();
  await axeRoutes(visitor, CLOUD, ['/fleet/signin', new URL(videosLink).pathname, new URL(neverLink).pathname]);

  // The take-home email itself, as a mail client shows it: a fresh page, so no site's CSP blocks its inline styles.
  const [mail] = await sentMail();
  const client = await (await browser.newContext()).newPage();
  await client.setContent(mail.html);
  expect(await seriousViolations(client), 'email').toEqual([]);
});

test('body text and the Clip that label reach 7:1; every court page action is at least 48 px (UX-A11Y)', async ({ browser }) => {
  const phone = await (await browser.newContext({ viewport: { width: 390, height: 844 } })).newPage();
  await phone.goto(`${PLAYER}/c/${code}`);
  expect(await contrast(phone.locator('main p').first()), 'body text').toBeGreaterThanOrEqual(7);
  expect(await contrast(phone.getByRole('button', { name: 'Clip that' })), 'Clip that label').toBeGreaterThanOrEqual(7);
  const targets = phone.locator('main button:visible, main a.btn:visible, main label.check:visible, main input:not([type=hidden]):visible');
  expect(await targets.count()).toBeGreaterThan(2);
  for (const t of await targets.all()) {
    const box = (await t.boundingBox())!;
    const name = (await t.getAttribute('aria-label')) ?? (await t.textContent())?.trim() ?? (await t.getAttribute('name'));
    expect(Math.min(box.width, box.height), `${name} is ${box.width}×${box.height}`).toBeGreaterThanOrEqual(48);
  }
});

test('with reduced motion the saving ring stands still and the countdown is text (UX-MOTION-KEYBOARD)', async ({ browser }) => {
  const phone = await (await browser.newContext({ reducedMotion: 'reduce' })).newPage();
  await phone.goto(`${PLAYER}/c/${code}`);
  await phone.getByRole('button', { name: 'Clip that' }).click();
  await expect(phone.locator('[data-saving-text]')).toHaveText(/^Saving… (ready in \d+ s|making your clip)$/);
  expect(await phone.locator('[data-ring]').evaluate((el) => getComputedStyle(el).animationName)).toBe('none');
  await expect(phone.locator('[data-clip-state="ready"]').first()).toBeVisible({ timeout: 60_000 });
});

test('SC-8 by keyboard alone: the Owner adds a staff member, then deactivates them (UX-MOTION-KEYBOARD)', async ({ browser }) => {
  const page = await owner(browser);
  await tabTo(page, page.getByRole('link', { name: 'Staff', exact: true }));
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${STAFF}/console/staff`);
  await tabTo(page, page.getByRole('link', { name: /Add staff/ }));
  await page.keyboard.press('Enter');
  await tabTo(page, page.getByLabel('Name', { exact: true }));
  await page.keyboard.type('Kim Keyboard');
  await tabTo(page, page.getByLabel('Username', { exact: true }));
  await page.keyboard.type('kim');
  await tabTo(page, page.getByRole('button', { name: 'Add staff and show temporary password' }));
  await page.keyboard.press('Enter');
  await expect(page.getByText(/temporary password/i).first()).toBeVisible();
  await page.goto(`${STAFF}/console/staff`);
  await tabTo(page, page.getByRole('link', { name: 'Edit Kim Keyboard' }));
  await page.keyboard.press('Enter');
  await tabTo(page, page.getByRole('button', { name: 'Deactivate Kim Keyboard' }));
  await page.keyboard.press('Enter');
  await expect(page.getByRole('status')).toContainText('Deactivated');
});

test('SC-6 by keyboard alone: filter clips the venue may post, keep some, download a vertical copy (UX-MOTION-KEYBOARD)', async ({ browser }) => {
  const page = await owner(browser);
  await tabTo(page, page.getByRole('link', { name: 'Clips', exact: true }));
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${STAFF}/console/clips`);
  await tabTo(page, page.getByLabel('Type'));
  await page.keyboard.type('Clips'); // a focused select picks the option starting with what is typed
  await expect(page.getByLabel('Type')).toHaveValue('tap');
  await tabTo(page, page.getByRole('button', { name: 'Show clips' }));
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(/kind=tap/);
  await expect(page.getByRole('link', { name: 'Clear filters' }).first()).toBeVisible();
  for (let i = 0; i < 2; i++) {
    const keep = page.getByRole('button', { name: /^Keep / }).first();
    await tabTo(page, keep);
    await page.keyboard.press('Enter');
    await expect(page.getByRole('status')).toBeVisible();
  }
  await expect(page.getByText(/^Kept until/).nth(1)).toBeVisible();
  await expect(page, 'Keep returns to the same filtered list').toHaveURL(/kind=tap.*notice=kept/);
  await tabTo(page, page.getByRole('link', { name: /^Download vertical / }).first());
  const [download] = await Promise.all([page.waitForEvent('download'), page.keyboard.press('Enter')]);
  expect(download.suggestedFilename()).toMatch(/\.mp4$/);
});

test('SC-7 by keyboard alone: find footage, hold it, export it (UX-MOTION-KEYBOARD)', async ({ browser }) => {
  const page = await owner(browser);
  await tabTo(page, page.getByRole('link', { name: 'Footage', exact: true }));
  await page.keyboard.press('Enter');
  await expect(page).toHaveURL(`${STAFF}/console/footage`);
  await tabTo(page, page.getByRole('button', { name: 'Find footage' }));
  await page.keyboard.press('Enter');
  await tabTo(page, page.getByRole('button', { name: 'Hold footage' }));
  await page.keyboard.press('Enter');
  await expect(page.getByRole('status')).toBeVisible();
  await tabTo(page, page.getByRole('link', { name: /^Export / }).first());
  const [download] = await Promise.all([page.waitForEvent('download', { timeout: 120_000 }), page.keyboard.press('Enter')]);
  expect(download.suggestedFilename()).toMatch(/\.mp4$/);
});

test('the clip library is a real table with a caption and scoped headers, pages of 50, and a delete confirmation naming the clip (UX-LIST-ARCHETYPE)', async ({ browser }) => {
  const page = await owner(browser);
  await page.goto(`${STAFF}/console/clips`);
  await expect(page.locator('table caption')).toContainText(/^1–50 of \d+ clips$/);
  expect(await page.locator('thead th').evaluateAll((ths) => ths.every((th) => th.getAttribute('scope') === 'col'))).toBe(true);
  await expect(page.locator('tbody tr')).toHaveCount(50);
  await page.getByRole('link', { name: 'Next 50' }).click();
  await expect(page.locator('table caption')).toContainText(/^51–/);
  await page.goto(`${STAFF}/console/clips?date=2001-01-01`);
  await expect(page.getByRole('heading', { name: 'No clips match these filters' })).toBeVisible();
  await page.goto(`${STAFF}/console/clips`);
  const del = page.getByRole('link', { name: /^Delete / }).first();
  const name = (await del.getAttribute('aria-label'))!.replace(/^Delete /, '');
  await del.click();
  await expect(page.getByRole('heading', { level: 1 })).toContainText(name);
  await page.getByRole('link', { name: 'Keep it' }).click();
});

test('both line drawings render, decode and carry the agreed alt text (UX-ASSETS)', async ({ browser }) => {
  const page = await owner(browser);
  await page.goto(`${STAFF}/setup/signs`);
  const img = page.locator('img.illustration');
  await expect(img).toHaveAttribute('alt', 'Line drawing of a pickleball court with the camera mounted high at the centre and the court sign on the net post');
  expect(await img.evaluate((i: HTMLImageElement) => i.decode().then(() => i.naturalWidth))).toBeGreaterThan(0);
  // The empty-library drawing renders on a box with no clips (onboard.spec.ts checks its alt text and decoding).
  const svg = await page.request.get(`${STAFF}/static/empty-library-scan.svg`);
  expect(svg.ok()).toBe(true);
  expect(svg.headers()['content-type']).toContain('image/svg+xml');
});

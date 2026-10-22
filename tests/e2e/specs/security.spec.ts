import { test, expect, type Page } from '@playwright/test';
import { CAMERA, PLAYER, STAFF, addCourt, completeSetup, courtCode, freshBox, startCameras, waitCourtStatus } from './helpers';

test.describe.configure({ mode: 'serial' });

// Each payload tries to run script (it would set window.__pwned) or break the SQL around it; all fit their fields.
const COURT = `Court<img src=x onerror=__pwned=1>'--`;
const STAFF_NAME = `<script>__pwned=1</script>Mallory'); DROP TABLE staff;--`;
const VENUE = `<svg onload=__pwned=1>Venue '); DROP TABLE staff;--`;
const CONTACT = `<img src=x onerror=__pwned=1>"><a href=javascript:__pwned=1>x`;

async function inert(page: Page, url: string, literal: string): Promise<void> {
  await page.goto(url);
  expect(await page.evaluate(() => (window as unknown as { __pwned?: number }).__pwned), `${url} ran a payload`).toBeUndefined();
  expect(await page.locator('body').textContent(), `${url} shows the text as typed`).toContain(literal);
  expect(await page.locator('script:not([src]), [onerror], [onload], a[href^="javascript:"]').count(), `${url} has no injected markup`).toBe(0);
}

test('script and SQL payloads in court, staff and settings fields render as text and change nothing else (SEC-INJECTION)', async ({ page, browser }) => {
  test.setTimeout(240_000);
  await freshBox();
  await startCameras('cam-3');
  await completeSetup(page);
  await addCourt(page, COURT, CAMERA(3));
  const code = await courtCode(page, COURT);
  await waitCourtStatus(page, COURT, /^Recording since/);

  await page.goto(`${STAFF}/console/staff/new`);
  await page.getByLabel('Name', { exact: true }).fill(STAFF_NAME);
  await page.getByLabel('Username', { exact: true }).fill('mallory');
  await page.getByRole('button', { name: 'Add staff and show temporary password' }).click();
  await expect(page.getByText(/temporary password/i).first()).toBeVisible();

  await page.goto(`${STAFF}/console/settings`);
  await page.getByLabel('Venue name').fill(VENUE);
  await page.getByLabel(/Venue contact/).fill(CONTACT);
  await page.getByRole('button', { name: 'Save settings' }).click();
  await expect(page.getByRole('status')).toHaveText('Settings saved.');

  for (const [url, literal] of [
    [`${STAFF}/console`, 'Court'],
    [`${STAFF}/console/courts`, 'Court'],
    [`${STAFF}/console/staff`, 'Mallory'],
    [`${STAFF}/console/settings`, 'Venue'],
    [`${STAFF}/console/signs`, 'Court'],
    // The log names a staff member by username; the payload it carries as typed is the venue name.
    [`${STAFF}/console/log`, 'Venue'],
    [`${PLAYER}/c/${code}`, 'Venue'],
    [`${PLAYER}/privacy`, 'Venue'],
  ] as const) {
    await inert(page, url, literal);
  }
  // The literal text, escaped, is what the pages carry.
  await page.goto(`${STAFF}/console/staff`);
  await expect(page.getByText(STAFF_NAME)).toBeVisible();
  // Every table is still there and still answers.
  await page.goto(`${STAFF}/console/staff`);
  await expect(page.locator('tbody tr')).toHaveCount(2);
  await page.goto(`${STAFF}/console/clips`);
  await expect(page.getByRole('heading', { level: 1 })).toHaveText('Clips');
});

import { test, expect } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { CAMERA, OWNER, STAFF, boxExec, compose, freshBox, signIn, startCameras } from './helpers';

test.describe.configure({ mode: 'serial' });

const LOGO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../fixtures/logo-magenta.png');

test('first run: the wizard shows field-level errors with a focused summary, then owner and venue (FR-40, design:states)', async ({ page }) => {
  await freshBox();
  await startCameras('cam-2', 'cam-3');
  compose('up', '-d', 'onvif-cam');
  await page.goto(`${STAFF}/console`);
  await expect(page).toHaveURL(`${STAFF}/setup`);
  await expect(page.locator('.steps [aria-current="step"]')).toHaveText('Owner and venue');
  // No demo content and no default account on a new box.
  expect(boxExec('ls /data/clips 2>/dev/null | wc -l').trim()).toBe('0');

  await page.getByRole('button', { name: 'Continue' }).click();
  const summary = page.locator('[data-error-summary]');
  await expect(summary).toBeVisible();
  await expect(summary).toBeFocused();
  for (const [field, text] of [['venue', "Enter the venue's name."], ['display_name', 'Enter your name.'], ['username', 'Choose a username'], ['password', 'at least 12 characters']]) {
    await expect(page.locator(`#${field}-error`)).toContainText(text);
    await expect(page.locator(`#${field}`)).toHaveAttribute('aria-invalid', 'true');
  }
  // The summary links take you to the field.
  await summary.getByRole('link', { name: "Enter the venue's name." }).click();
  await expect(page).toHaveURL(/#venue$/);

  // The viewport never locks zoom, and password managers can fill the new password.
  expect(await page.locator('meta[name="viewport"]').getAttribute('content')).not.toMatch(/maximum-scale|user-scalable=no/);
  await expect(page.getByLabel('Owner password')).toHaveAttribute('autocomplete', 'new-password');

  await page.getByLabel('Venue name').fill(OWNER.venue);
  await page.getByLabel(/Venue logo/).setInputFiles(LOGO);
  await page.getByLabel('Opens').fill('00:00');
  await page.getByLabel('Closes').fill('23:59');
  await page.getByLabel('Your name').fill(OWNER.name);
  await page.getByLabel('Owner username').fill(OWNER.username);
  await page.getByLabel('Owner password').fill(OWNER.password);
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page).toHaveURL(`${STAFF}/setup/courts`);
  await expect(page.locator('.steps [aria-current="step"]')).toHaveText('Courts and cameras');
});

test('step 2 finds the camera on the network, asks it for its stream with its password, previews it; another is added by address (FR-30, FR-40)', async ({ page }) => {
  await signIn(page, OWNER.username, OWNER.password);
  await page.goto(`${STAFF}/setup/courts`);
  const found = page.getByRole('radio', { name: /Court Cam · TestCam-4K/ });
  await expect(found).toBeVisible();

  await page.getByLabel('Court name').fill('Court 1');
  await found.check();
  await page.getByLabel('Camera username').fill('admin');
  await page.getByLabel('Camera password').fill('wrong-password');
  await page.getByRole('button', { name: 'Add court' }).click();
  await expect(page.locator('#found-error')).toHaveText('The camera refused that username or password.');
  await expect(page.locator('[data-error-summary]')).toBeFocused();

  await page.getByRole('radio', { name: /Court Cam · TestCam-4K/ }).check();
  await page.getByLabel('Camera username').fill('admin');
  await page.getByLabel('Camera password').fill('cam-pass-1');
  await page.getByRole('button', { name: 'Add court' }).click();
  await expect(page).toHaveURL(`${STAFF}/setup/courts`);
  const court1 = page.locator('li[data-court="Court 1"]');
  await expect(court1).toBeVisible();
  // The camera's own answer filled in the address; the court records it.
  await page.goto(`${STAFF}/console/courts`);
  await page.getByRole('link', { name: 'Edit Court 1' }).click();
  await expect(page.getByLabel('Camera stream address')).toHaveValue('rtsp://rtsp:8554/court3');

  await page.goto(`${STAFF}/setup/courts`);
  await page.getByLabel('Court name').fill('Court 2');
  await page.getByRole('radio', { name: /None of these/ }).check();
  await page.getByLabel('Camera stream address').fill(CAMERA(2));
  await page.getByRole('button', { name: 'Add court' }).click();
  await expect(page.locator('li[data-court="Court 2"]')).toBeVisible();

  // Live previews once each court records.
  await expect.poll(async () => {
    await page.goto(`${STAFF}/setup/courts`);
    const decoded = await page.locator('li[data-court] img.preview').evaluateAll((imgs) => Promise.all(imgs.map((i) => (i as HTMLImageElement).decode().then(() => 1, () => 0))));
    return decoded.reduce((a: number, b: number) => a + b, 0);
  }, { timeout: 60_000, intervals: [3000] }).toBe(2);
});

test('step 3 prints the signs; Back returns to courts; Finish opens the empty library that explains the first clip (FR-37, FR-40)', async ({ page }) => {
  await signIn(page, OWNER.username, OWNER.password);
  await page.goto(`${STAFF}/setup/courts`);
  await page.getByRole('link', { name: 'Continue to court signs' }).click();
  await expect(page.locator('.steps [aria-current="step"]')).toHaveText('Court signs');
  await expect(page.getByText('(2 courts)')).toBeVisible();
  await page.getByRole('link', { name: '← Back: courts and cameras' }).click();
  await expect(page).toHaveURL(`${STAFF}/setup/courts`);
  await page.getByRole('link', { name: 'Continue to court signs' }).click();
  await expect(page.getByRole('link', { name: 'Print court signs' })).toHaveAttribute('href', '/console/signs');
  await page.getByRole('link', { name: 'Finish setup' }).click();
  await expect(page).toHaveURL(`${STAFF}/console/clips`);
  await expect(page.getByRole('heading', { name: 'No clips yet — players tap Clip that on the court page' })).toBeVisible();
  const drawing = page.locator('[data-illustration="scan-code"]');
  await expect(drawing).toHaveAttribute('alt', 'Line drawing of a phone scanning the court code on a net post');
  expect(await drawing.evaluate((i: HTMLImageElement) => i.naturalWidth)).toBeGreaterThan(0);
});

test('courts are renamed and deleted from the console; form problems show under the field (FR-30, design:states)', async ({ page }) => {
  await signIn(page, OWNER.username, OWNER.password);
  await page.goto(`${STAFF}/console/courts`);
  await page.getByRole('link', { name: 'Edit Court 2' }).click();
  await page.getByLabel('Camera stream address').fill('http://not-a-camera');
  await page.getByRole('button', { name: 'Save court' }).click();
  await expect(page.locator('#camera_url-error')).toContainText('starts with rtsp://');
  await expect(page.locator('[data-error-summary]')).toBeFocused();
  await page.getByLabel('Court name').fill('Court 2 East');
  await page.getByLabel('Camera stream address').fill(CAMERA(2));
  await page.getByRole('button', { name: 'Save court' }).click();
  await expect(page.getByRole('status')).toContainText('Saved');
  await expect(page.getByRole('link', { name: 'Edit Court 2 East' })).toBeVisible();
  await page.getByRole('link', { name: 'Edit Court 2 East' }).click();
  await page.getByRole('link', { name: 'Delete Court 2 East' }).click();
  await page.getByRole('button', { name: 'Delete Court 2 East' }).click();
  await expect(page.getByRole('status')).toContainText('Court deleted');
  await expect(page.getByRole('link', { name: 'Edit Court 2 East' })).toHaveCount(0);

  // Settings: the problem shows under its field, with the summary focused.
  await page.goto(`${STAFF}/console/settings`);
  await page.getByLabel(/Upload cap/).fill('0');
  await page.getByRole('button', { name: 'Save settings' }).click();
  await expect(page.locator('#cap-error')).toHaveText('Upload cap must be between 1 and 1000 Mbit/s.');
  await expect(page.locator('[data-error-summary]')).toBeFocused();
});

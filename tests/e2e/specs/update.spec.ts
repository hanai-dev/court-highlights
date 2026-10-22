import { test, expect, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CAMERA, CLOUD, OWNER, PLAYER, STAFF, addCourt, boxExec, cloudUp, compose, completeSetup, connectBox, createAdmin, fleetSignIn, freshBox, signIn,
  startCameras, waitCourtStatus,
} from './helpers';

test.describe.configure({ mode: 'serial' });

// Update files are made inside the box container (it has the built ch-box) and land here through the /work mount.
const OUT = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../test-results/updates');
const file = (name: string): string => path.join(OUT, name);
const KEY = '/work/tests/fixtures/release-test.pk8'; // test-only signing key; the boxes trust it via CH_RELEASE_KEY

function makeUpdate(name: string, version: string, payload: string): void {
  // Made here first so the folder belongs to the test runner, not to the container's root (Linux bind mounts).
  fs.mkdirSync(OUT, { recursive: true });
  boxExec(`/work/target/debug/ch-box make-update ${version} ${payload} ${KEY} /work/tests/e2e/test-results/updates/${name}`);
}

async function boxBack(): Promise<void> {
  await expect.poll(async () => (await fetch(`${PLAYER}/healthz`).catch(() => null))?.ok ?? false, { timeout: 120_000, intervals: [2000] }).toBe(true);
}

async function updatePage(page: Page): Promise<void> {
  await signIn(page, OWNER.username, OWNER.password);
  await page.goto(`${STAFF}/console/settings/update`);
}

test('an update from a USB stick installs, proves itself and stays; a damaged file installs nothing (FR-54)', async ({ page }) => {
  test.setTimeout(300_000);
  await freshBox();
  await cloudUp();
  await startCameras('cam-3');
  await completeSetup(page);
  await connectBox(page);
  await addCourt(page, 'Court 1', CAMERA(3));
  await waitCourtStatus(page, 'Court 1', /^Recording since/);

  boxExec(`printf '#!/bin/sh\\necho broken release >&2\\nexit 1\\n' > /tmp/crash && printf '#!/bin/sh\\nexec sleep 100000\\n' > /tmp/hang && chmod 755 /tmp/crash /tmp/hang`);
  makeUpdate('good.chupdate', '0.1.1-test', '/work/target/debug/ch-box');
  makeUpdate('crash.chupdate', '0.1.2-crash', '/tmp/crash');
  makeUpdate('hang.chupdate', '0.1.3-hang', '/tmp/hang');
  const tampered = fs.readFileSync(file('good.chupdate'));
  tampered[tampered.length - 1] ^= 1;
  fs.writeFileSync(file('tampered.chupdate'), tampered);

  await page.goto(`${STAFF}/console/settings`);
  await page.getByRole('link', { name: 'Software update…' }).click();
  const before = (await page.locator('[data-box-version]').textContent())!.trim();
  await page.getByLabel('Update file').setInputFiles(file('tampered.chupdate'));
  await page.getByRole('button', { name: 'Install update' }).click();
  await expect(page.locator('[data-error-summary]')).toContainText("its contents don't match");
  await expect(page.locator('[data-box-version]')).toHaveText(before);

  await page.getByLabel('Update file').setInputFiles(file('good.chupdate'));
  await page.getByRole('button', { name: 'Install update' }).click();
  await expect(page.locator('[data-update-installing]')).toContainText('Installing version 0.1.1-test');
  await page.waitForTimeout(5_000);
  await boxBack();
  await expect.poll(async () => {
    await updatePage(page);
    return (await page.locator('[data-update-status]').textContent({ timeout: 1000 }).catch(() => '')) ?? '';
  }, { timeout: 90_000, intervals: [3000] }).toBe('Updated to 0.1.1-test.');
  await expect(page.locator('[data-box-version]')).toHaveText('0.1.1-test');
  await waitCourtStatus(page, 'Court 1', /^Recording since/);
});

test('a failing update from a USB stick returns the box to its previous version by itself and says so (FR-54, NFR-12)', async ({ page }) => {
  test.setTimeout(300_000);
  await updatePage(page);
  await page.getByLabel('Update file').setInputFiles(file('crash.chupdate'));
  await page.getByRole('button', { name: 'Install update' }).click();
  await expect(page.locator('[data-update-installing]')).toContainText('Installing version 0.1.2-crash');
  const installed = Date.now();
  await page.waitForTimeout(5_000);
  await boxBack();
  await expect.poll(async () => {
    await updatePage(page);
    return (await page.locator('[data-update-status]').textContent({ timeout: 1000 }).catch(() => '')) ?? '';
  }, { timeout: 120_000, intervals: [3000] }).toBe('Update rolled back: version 0.1.2-crash did not start.');
  expect(Date.now() - installed, 'back on the previous version within 10 minutes').toBeLessThan(10 * 60_000);
  await expect(page.locator('[data-box-version]')).toHaveText('0.1.1-test');
  await waitCourtStatus(page, 'Court 1', /^Recording since/);
  await page.goto(`${STAFF}/console/log`);
  await expect(page.locator('tbody tr').filter({ hasText: 'Update rolled back' }).first()).toContainText('0.1.2-crash');
});

test('a failing update from the fleet goes to one box of a venue at a time; it rolls back and the venue\'s other boxes never get it (FR-51, SC-9)', async ({ page, browser }) => {
  test.setTimeout(420_000);
  // A second box at the same venue, checking in through the box API.
  const otherKey = compose('exec', '-T', 'cloud', './target/debug/ch-cloud', 'add-box', OWNER.venue, 'box-z').trim();
  const otherCheckin = async (): Promise<{ update?: { version: string } }> =>
    (await (await fetch(`${CLOUD}/api/v1/checkin`, {
      method: 'POST',
      headers: { Authorization: `Bearer ${otherKey}`, 'content-type': 'application/json' },
      body: JSON.stringify({ version: '0.1.1-test', disk_pct: 5, courts_recording: 1, courts_total: 1 }),
    })).json()) as { update?: { version: string } };

  const admin = createAdmin('releaser');
  const fleet = await (await browser.newContext()).newPage();
  await fleetSignIn(fleet, admin);
  await fleet.getByLabel('Temporary password').fill(admin.password);
  await fleet.getByLabel('New password').fill('releaser-pass-2026');
  await fleet.getByRole('button', { name: 'Change password' }).click();
  await fleet.goto(`${CLOUD}/fleet/updates`);
  await fleet.getByLabel('Update file').setInputFiles(file('hang.chupdate'));
  await fleet.getByRole('button', { name: 'Publish update' }).click();
  await expect(fleet.locator('tr[data-release="0.1.3-hang"]')).toBeVisible();

  // box-a checks in every 3 s, takes the offer and starts installing; meanwhile box-z is not offered it.
  await expect.poll(async () => {
    await fleet.goto(`${CLOUD}/fleet/updates`);
    return (await fleet.locator('tr[data-box-update="box-a"]').first().textContent({ timeout: 1000 }).catch(() => '')) ?? '';
  }, { timeout: 60_000, intervals: [2000] }).toContain('Installing');
  expect((await otherCheckin()).update, 'one box of a venue at a time').toBeUndefined();

  // The new version never becomes healthy: the launcher takes it back after the grace period.
  await page.waitForTimeout(10_000);
  await boxBack();
  await expect.poll(async () => {
    await updatePage(page);
    return (await page.locator('[data-update-status]').textContent({ timeout: 1000 }).catch(() => '')) ?? '';
  }, { timeout: 240_000, intervals: [5000] }).toMatch(/^Update rolled back: version 0\.1\.3-hang not healthy within \d+ s\.$/);
  await expect(page.locator('[data-box-version]')).toHaveText('0.1.1-test');
  await expect.poll(async () => {
    await fleet.goto(`${CLOUD}/fleet/updates`);
    return (await fleet.locator('tr[data-box-update="box-a"]').first().textContent({ timeout: 1000 }).catch(() => '')) ?? '';
  }, { timeout: 60_000, intervals: [3000] }).toContain('Update rolled back');
  expect((await otherCheckin()).update, "the venue's other boxes are not updated").toBeUndefined();
  await fleet.goto(`${CLOUD}/fleet/log`);
  await expect(fleet.locator('tbody tr').filter({ hasText: 'update rolled back' }).first()).toContainText('box-a');
  await expect(fleet.locator('tbody tr').filter({ hasText: 'published update' }).first()).toContainText('0.1.3-hang');
});

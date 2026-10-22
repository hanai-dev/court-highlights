import { test, expect, type Page } from '@playwright/test';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import {
  CAMERA, OWNER, PLAYER, STAFF, addCourt, boxExec, completeSetup, freshBox, signIn,
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
  await startCameras('cam-3');
  await completeSetup(page);
  await addCourt(page, 'Court 1', CAMERA(3));
  await waitCourtStatus(page, 'Court 1', /^Recording since/);

  boxExec(`printf '#!/bin/sh\\necho broken release >&2\\nexit 1\\n' > /tmp/crash && chmod 755 /tmp/crash`);
  makeUpdate('good.chupdate', '0.1.1-test', '/work/target/debug/ch-box');
  makeUpdate('crash.chupdate', '0.1.2-crash', '/tmp/crash');
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

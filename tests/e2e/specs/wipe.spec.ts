import { test, expect } from '@playwright/test';
import { CAMERA, OWNER, PLAYER, STAFF, addCourt, boxExec, completeSetup, courtCode, freshBox, signIn, startCameras, waitCourtStatus } from './helpers';

test('Wipe box: the Owner types the venue name; clips, footage, accounts and logs are gone from the disk (FR-55, SEC-WIPE)', async ({ page, browser }) => {
  test.setTimeout(240_000);
  await freshBox();
  await startCameras('cam-3');
  await completeSetup(page);
  await addCourt(page, 'Court 1', CAMERA(3));
  const code = await courtCode(page, 'Court 1');
  await waitCourtStatus(page, 'Court 1', /^Recording since/);
  const p = await (await browser.newContext()).newPage();
  await p.goto(`${PLAYER}/c/${code}`);
  await p.getByRole('button', { name: 'Clip that' }).click();
  await expect(p.locator('[data-clip-state="ready"]').first()).toBeVisible({ timeout: 60_000 });
  expect(Number(boxExec('find /data -name "*.mp4" | wc -l').trim())).toBeGreaterThan(0);

  const staff = await (await browser.newContext({ ignoreHTTPSErrors: true })).newPage();
  await signIn(staff, OWNER.username, OWNER.password);
  await staff.goto(`${STAFF}/console/settings`);
  await staff.getByRole('link', { name: 'Wipe this box…' }).click();
  await staff.getByLabel(/Type the venue name to confirm/).fill('Some other venue');
  await staff.getByRole('button', { name: 'Wipe this box' }).click();
  await expect(staff.locator('#confirm-error')).toContainText(`Type the venue name exactly as shown: ${OWNER.venue}`);
  expect(Number(boxExec('find /data -name "*.mp4" | wc -l').trim()), 'a wrong name erases nothing').toBeGreaterThan(0);

  await staff.getByLabel(/Type the venue name to confirm/).fill(OWNER.venue);
  await staff.getByRole('button', { name: 'Wipe this box' }).click();
  await expect(staff.getByRole('heading', { name: 'This box is wiped' })).toBeVisible();
  // The box process ends and its launcher (systemd on the box) starts it again: a new, empty box at setup.
  await expect.poll(async () => {
    const res = await staff.request.get(`${STAFF}/console`).catch(() => null);
    return res ? new URL(res.url()).pathname : '';
  }, { timeout: 60_000, intervals: [2000] }).toBe('/setup');
  await staff.goto(`${STAFF}/console`);
  await expect(staff).toHaveURL(`${STAFF}/setup`);
  const files = boxExec('find /data -type f').trim().split('\n');
  expect(files.filter((f) => /\.(mp4|jpg|png|ts|mkv)$/.test(f)), 'no footage, clips or logos').toEqual([]);
  for (const text of [OWNER.venue, OWNER.username, 'argon2id', code]) {
    expect(boxExec(`grep -r -a -l "${text}" /data || true`).trim(), `nothing on the disk mentions ${text}`).toBe('');
  }
});

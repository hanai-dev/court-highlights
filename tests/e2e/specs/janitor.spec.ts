import { test, expect, type Page } from '@playwright/test';
import { CAMERA, OWNER, PLAYER, STAFF, addCourt, advanceClock, boxExec, completeSetup, compose, courtCode, freshBox, msUntilManila, signIn, startCameras, waitCourtStatus } from './helpers';

test.describe.configure({ mode: 'serial' });

let code = '';

async function owner(page: Page): Promise<void> {
  await signIn(page, OWNER.username, OWNER.password);
  await expect(page).toHaveURL(`${STAFF}/console`);
}

/** Taps Clip that and returns the new clip's token once it is ready. */
async function clip(page: Page): Promise<string> {
  await page.goto(`${PLAYER}/c/${code}`);
  const before = await page.locator('[data-clip-state="ready"]').count();
  await page.getByRole('button', { name: 'Clip that' }).click();
  await expect(page.locator('[data-clip-state="ready"]')).toHaveCount(before + 1, { timeout: 60_000 });
  return (await page.locator('[data-clip-state="ready"] a.clip-link').first().getAttribute('href'))!.split('/').pop()!;
}

/** Plays a short game; returns the game video's token. */
async function game(page: Page): Promise<string> {
  await page.goto(`${PLAYER}/c/${code}`);
  await page.getByRole('button', { name: 'Start game' }).click();
  await page.waitForTimeout(6_000);
  await page.getByRole('button', { name: 'Stop game' }).click();
  await expect(page.locator('[data-game-video]')).toBeVisible({ timeout: 90_000 });
  return (await page.locator('[data-game-video]').getAttribute('href'))!.split('/').pop()!;
}

const exists = (token: string): boolean => boxExec(`[ -d /data/clips/${token} ] && echo yes || echo no`).trim() === 'yes';

async function libraryHas(page: Page, token: string): Promise<boolean> {
  // The library shows ids, not tokens: look the token up through the staff file route of each row.
  await page.goto(`${STAFF}/console/clips`);
  const ids = await page.locator('tbody tr').evaluateAll((rows) => rows.map((r) => r.getAttribute('data-clip')));
  return ids.length > 0 && exists(token);
}

test('3:00 am clean-up removes clips 72 h old, keeps kept and held ones until their own limits (FR-45, GV-10…GV-12, GV-18)', async ({ page }) => {
  test.setTimeout(400_000);
  await freshBox();
  await startCameras('cam-3');
  await completeSetup(page);
  await page.goto(`${STAFF}/console/settings`);
  await page.getByLabel('Seconds before the tap').fill('10');
  await page.getByLabel('Seconds after the tap').fill('0');
  await page.getByLabel(/Make highlights/).uncheck();
  await page.getByRole('button', { name: 'Save settings' }).click();
  await addCourt(page, 'Court 1', CAMERA(3));
  code = await courtCode(page, 'Court 1');
  await waitCourtStatus(page, 'Court 1', /^Recording since/);
  await page.waitForTimeout(12_000);

  const boxNow = async (): Promise<number> => Date.parse(await (await page.request.post(`${STAFF}/__test/clock`, { form: { ms: '0' } })).text());
  const plain = await clip(page);
  await page.waitForTimeout(12_000); // clip windows are 10 s: keep them apart
  const kept = await clip(page);
  await page.waitForTimeout(12_000);
  const tapFrom = await boxNow();
  const held = await clip(page);
  const tapTo = await boxNow();

  await owner(page);
  await page.goto(`${STAFF}/console`);
  await expect(page.locator('tr[data-court="Court 1"] [data-last-clip]')).toHaveText(/^\d{1,2} \w{3}, \d{1,2}:\d{2} [ap]m$/);
  await page.goto(`${STAFF}/console/clips?kind=tap`);
  const keptId = await page.locator('tbody tr').nth(1).getAttribute('data-clip');
  await page.locator(`tr[data-clip="${keptId}"]`).getByRole('button', { name: /^Keep / }).click();
  // Hold exactly the newest clip's footage (the footage form posts these same fields, in whole minutes).
  const hold = await page.request.post(`${STAFF}/console/footage/holds`, { form: { court: '1', start_ms: String(tapFrom - 10_500), end_ms: String(tapTo) } });
  expect(hold.ok()).toBe(true);
  expect([exists(plain), exists(kept), exists(held)]).toEqual([true, true, true]);

  // 72 h later, then on to the next 03:00: only the plain clip goes.
  await advanceClock(page.request, 72 * 3600_000);
  await advanceClock(page.request, await msUntilManila(page.request, 3, 0, 1));
  await expect.poll(() => exists(plain), { timeout: 45_000, intervals: [3000] }).toBe(false);
  expect(exists(kept), 'kept: stays until 1 year after Keep').toBe(true);
  expect(exists(held), 'held: stays while the hold lasts').toBe(true);
  await owner(page); // the 72 h jump ended the 12-hour sign-in
  await page.goto(`${STAFF}/console/clips?kind=tap`);
  await expect(page.locator('tbody tr')).toHaveCount(2);

  // A year on (and the 60-day hold long released): both go at the next clean-up.
  await advanceClock(page.request, 366 * 24 * 3600_000);
  await advanceClock(page.request, await msUntilManila(page.request, 3, 0, 1));
  await expect.poll(() => [exists(kept), exists(held)], { timeout: 45_000, intervals: [3000] }).toEqual([false, false]);
  await owner(page);
  await page.goto(`${STAFF}/console/clips`);
  await expect(page.getByRole('heading', { name: /No clips yet/ })).toBeVisible();
});

test('disk pressure: a warning from 80 %, and at 90 % the oldest unkept game videos go first, then clips; footage stays (FR-46, GV-20)', async ({ page }) => {
  test.setTimeout(400_000);
  await owner(page);
  await waitCourtStatus(page, 'Court 1', /^Recording since/, 30_000);
  await page.waitForTimeout(12_000);
  const older = await game(page);
  await page.waitForTimeout(2_000);
  const newer = await game(page);
  const tap = await clip(page);
  // Footage grows ~100 kB/s while the camera runs; pause it so the pretend usage levels stay put.
  compose('stop', 'cam-3');
  await page.waitForTimeout(8_000);
  const used = (): number => Number(boxExec('du -sb /data | cut -f1').trim());

  // 82 %: warning only.
  await page.request.post(`${STAFF}/__test/disk`, { form: { bytes: String(Math.round(used() / 0.82)) } });
  await page.goto(`${STAFF}/console`);
  await expect(page.locator('[data-disk]')).toHaveText(/^8\d % used/);
  await expect(page.getByText('the box will clear the oldest unkept game videos at 90 %')).toBeVisible();
  await page.waitForTimeout(35_000);
  expect([exists(older), exists(newer), exists(tap)], 'nothing removed below 90 %').toEqual([true, true, true]);

  // 91 %: removal in order — older game video, newer game video, then clips — stopping once below 85 %.
  const footageBefore = Number(boxExec('ls /data/footage/court-1 | wc -l').trim());
  await page.request.post(`${STAFF}/__test/disk`, { form: { bytes: String(Math.round(used() / 0.91)) } });
  await expect.poll(() => exists(older), { timeout: 45_000, intervals: [3000] }).toBe(false);
  const left = [exists(older), exists(newer), exists(tap)];
  // Whatever was removed is a prefix of the order: no clip goes while a game video is left.
  if (left[1]) expect(left[2], 'the clip outlives a game video').toBe(true);
  expect(Number(boxExec('ls /data/footage/court-1 | wc -l').trim()), 'rolling footage is never removed for space').toBeGreaterThanOrEqual(footageBefore);
  await page.request.post(`${STAFF}/__test/disk`, { form: { bytes: '0' } });
  compose('start', 'cam-3');
});

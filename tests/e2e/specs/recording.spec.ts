import { test, expect } from '@playwright/test';
import { CAMERA, OWNER, STAFF, addCourt, signIn, advanceClock, boxExec, compose, completeSetup, freshBox, msUntilManila, startCameras, waitCourtStatus } from './helpers';

test.describe.configure({ mode: 'serial' });

interface Segment {
  path: string;
  seconds: number;
}

function segments(court: number): Segment[] {
  const out = boxExec(`for f in /data/footage/court-${court}/*.mp4; do [ -e "$f" ] && echo "$f $(ffprobe -v error -show_entries format=duration -of csv=p=0 "$f")"; done; true`);
  return out
    .split('\n')
    .filter((l) => l.trim())
    .map((l) => {
      const [path, seconds] = l.trim().split(' ');
      return { path, seconds: Number(seconds) };
    })
    .filter((s) => Number.isFinite(s.seconds));
}

function streams(file: string): string {
  return boxExec(`ffprobe -v error -show_entries stream=codec_type,codec_name,profile,width,height -of csv=p=0 "${file}"`).trim();
}

test('courts are added with cameras and record by stream copy, without audio (FR-1, FR-2, FR-30)', async ({ page }) => {
  await freshBox();
  await startCameras();
  await completeSetup(page);
  await addCourt(page, 'Court 1', CAMERA(1));
  await addCourt(page, 'Court 2', CAMERA(2));
  await waitCourtStatus(page, 'Court 1', /^Recording since/);
  await waitCourtStatus(page, 'Court 2', /^Recording since/);

  await expect.poll(() => segments(1).filter((s) => s.seconds > 8).length, { timeout: 45_000, intervals: [2000] }).toBeGreaterThanOrEqual(2);
  const seg = segments(1).find((s) => s.seconds > 8)!;
  expect(seg.seconds).toBeLessThan(13);
  const recorded = streams(seg.path);
  const source = boxExec(`ffprobe -v error -rtsp_transport tcp -show_entries stream=codec_type,codec_name,profile,width,height -of csv=p=0 ${CAMERA(1)}`).trim();
  expect(source, 'the fake camera offers audio').toContain('audio');
  const sourceVideo = source.split('\n').find((l) => l.includes('video'));
  expect(recorded, 'stream copy: same codec, profile and size; no audio track').toBe(sourceVideo);

  // Live preview still on the court page.
  await page.goto(`${STAFF}/console/courts`);
  await page.getByRole('link', { name: 'Edit Court 1' }).click();
  const still = page.getByRole('img', { name: /Latest still from the Court 1 camera/ });
  await expect(still).toBeVisible();
  await expect.poll(() => still.evaluate((img: HTMLImageElement) => img.naturalWidth)).toBe(640);
});

test('an unplugged camera turns its court red within 10 s and recording resumes within 5 s of its return (FR-3)', async ({ page }) => {
  await signIn(page, OWNER.username, OWNER.password);
  compose('stop', 'cam-2');
  const redAfter = await waitCourtStatus(page, 'Court 2', /^Camera offline since/, 15_000);
  expect(redAfter, 'red within 10 s').toBeLessThanOrEqual(10_000);

  compose('start', 'cam-2');
  await expect
    .poll(() => {
      try {
        boxExec(`ffprobe -v error -rtsp_transport tcp -show_entries stream=codec_name -of csv=p=0 ${CAMERA(2)}`);
        return true;
      } catch {
        return false;
      }
    }, { timeout: 20_000, intervals: [250] })
    .toBe(true);
  const resumedAfter = await waitCourtStatus(page, 'Court 2', /^Recording since/, 15_000);
  expect(resumedAfter, 'recording resumes ≤ 5 s after the stream returns').toBeLessThanOrEqual(5_000);
});

test('the box keeps only the last 4 hours of footage (GV-13)', async ({ page, request }) => {
  await signIn(page, OWNER.username, OWNER.password);
  await expect.poll(() => segments(1).length, { timeout: 30_000 }).toBeGreaterThanOrEqual(2);
  const before = segments(1).map((s) => s.path);
  await advanceClock(request, 4 * 3600_000 + 60_000);
  await expect.poll(() => segments(1).filter((s) => before.includes(s.path)).length, { timeout: 90_000, intervals: [3000] }).toBe(0);
  await waitCourtStatus(page, 'Court 1', /^Recording since/);
  await expect.poll(() => segments(1).length, { timeout: 40_000, intervals: [2000] }).toBeGreaterThanOrEqual(1);
});

test('outside opening hours nothing is recorded; recording starts again at opening (FR-1, GV-21)', async ({ page, request }) => {
  await freshBox();
  await startCameras('cam-2');
  await completeSetup(page, undefined, ['06:00', '23:00']);
  await advanceClock(request, await msUntilManila(request, 23, 0, 5));
  await addCourt(page, 'Court 2', CAMERA(2));
  await waitCourtStatus(page, 'Court 2', /^Closed/);
  await new Promise((r) => setTimeout(r, 6000));
  expect(segments(1).length + segments(2).length, 'no footage while closed').toBe(0);
  await advanceClock(request, await msUntilManila(request, 6, 0, 1));
  await waitCourtStatus(page, 'Court 2', /^Recording since/);
});

test('a court is deleted only after a confirmation that names it (FR-30)', async ({ page }) => {
  await signIn(page, OWNER.username, OWNER.password);
  await page.goto(`${STAFF}/console/courts`);
  await page.getByRole('link', { name: 'Edit Court 2' }).click();
  await page.getByRole('link', { name: 'Delete Court 2' }).click();
  await expect(page.getByRole('heading', { name: 'Delete Court 2?' })).toBeVisible();
  await expect(page.getByText("can't be undone")).toBeVisible();
  await page.getByRole('button', { name: 'Delete Court 2' }).click();
  await expect(page.getByRole('status')).toContainText('Court deleted');
  await expect(page.getByRole('heading', { name: 'No courts yet' })).toBeVisible();
});

import { test, expect, webkit, chromium, devices, type Browser, type Page } from '@playwright/test';
import path from 'node:path';
import { fileURLToPath } from 'node:url';
import { CAMERA, OWNER, PLAYER, addCourt, advanceClock, boxExec, completeSetup, courtCode, freshBox, startCameras, waitCourtStatus } from './helpers';

test.describe.configure({ mode: 'serial' });

const LOGO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '../../fixtures/logo-magenta.png');
let code = '';
let otherCode = '';

interface Probe {
  width: number;
  height: number;
  seconds: number;
  streams: string[];
}

function probe(file: string): Probe {
  const out = boxExec(`ffprobe -v error -show_entries stream=codec_type,width,height:format=duration -of json "${file}"`);
  const j = JSON.parse(out) as { streams: { codec_type: string; width?: number; height?: number }[]; format: { duration: string } };
  const video = j.streams.find((s) => s.codec_type === 'video')!;
  return { width: video.width!, height: video.height!, seconds: Number(j.format.duration), streams: j.streams.map((s) => s.codec_type) };
}

/** Average RGB of a box in the frame 1 s into the file. */
function colour(file: string, w: number, h: number, x: number, y: number): number[] {
  const out = boxExec(
    `ffmpeg -v error -ss 1 -i "${file}" -frames:v 1 -vf crop=${w}:${h}:${x}:${y},scale=1:1:flags=area -f rawvideo -pix_fmt rgb24 - | od -An -tu1`,
  );
  return out.trim().split(/\s+/).map(Number);
}

const magenta = (rgb: number[]): boolean => rgb[0] > 190 && rgb[1] < 70 && rgb[2] > 190;

function clipFiles(token: string): { wide: string; vertical: string } {
  return { wide: `/data/clips/${token}/wide.mp4`, vertical: `/data/clips/${token}/vertical.mp4` };
}

async function readyClips(page: Page): Promise<string[]> {
  await page.goto(`${PLAYER}/c/${code}`);
  const hrefs = await page.locator('[data-clip-state="ready"] a.clip-link').evaluateAll((as) => as.map((a) => a.getAttribute('href') ?? ''));
  return hrefs.map((h) => h.split('/').pop()!);
}

test('the court page opens without sign-in and shows its parts in order (FR-17)', async ({ page }) => {
  await freshBox();
  await startCameras('cam-3');
  await completeSetup(page, OWNER, ['00:00', '23:59'], LOGO);
  await addCourt(page, 'Court 3', CAMERA(3));
  await addCourt(page, 'Court 9', ''); // a court with no camera: its code must not reach Court 3's clips
  code = await courtCode(page, 'Court 3');
  otherCode = await courtCode(page, 'Court 9');
  expect(code).toMatch(/^[A-Z2-9]{8}$/);
  await waitCourtStatus(page, 'Court 3', /^Recording since/);

  const player = await (await page.context().browser()!.newContext()).newPage();
  await player.goto(`${PLAYER}/c/${code.toLowerCase()}`);
  await expect(player.getByRole('img', { name: OWNER.venue })).toBeVisible();
  await expect(player.getByRole('heading', { level: 1 })).toHaveText('Court 3');
  await expect(player.locator('[data-court-status]')).toHaveText('Recording');
  await expect(player.getByRole('button', { name: 'Clip that' })).toBeEnabled();
  // Document order of the parts (A-112); game and email panels join this list with F-GAME / F-EMAIL.
  const order = await player.locator('img.venue-logo, h1, [data-court-status], .btn-clip-that, #clips-h').evaluateAll((els) =>
    els.map((e) => e.getAttribute('data-court-status') !== null ? 'status' : (e.className || e.tagName.toLowerCase())),
  );
  expect(order).toEqual(['venue-logo', 'court', 'status', 'btn-clip-that', 'h2']);
  const box = await player.getByRole('button', { name: 'Clip that' }).boundingBox();
  expect(box!.height).toBeGreaterThanOrEqual(48);

  await player.goto(`${PLAYER}/c/${otherCode}`);
  await expect(player.locator('[data-court-status]')).toContainText('Camera offline');
  await expect(player.getByRole('button', { name: 'Clip that' })).toBeDisabled();
  const refused = await player.request.post(`${PLAYER}/c/${otherCode}/tap`, { headers: { Accept: 'application/json', Origin: PLAYER } });
  expect(refused.status()).toBe(409);
});

test('Clip that saves 25 s before to 5 s after the tap, in both shapes, with the logo and no audio (FR-4, FR-6, FR-7, FR-8, FR-2)', async ({ page }) => {
  await page.waitForTimeout(30_000); // let 30 s of footage build up so the window is not clamped
  await page.goto(`${PLAYER}/c/${code}`);
  await page.getByRole('button', { name: 'Clip that' }).click();
  await expect(page.locator('[data-saving]')).toBeVisible();
  await expect(page.locator('[data-saving-text]')).toHaveText(/Saving… ready in [1-5] s/);
  await expect(page.locator('[data-clip-state="saving"]')).toHaveCount(1);
  await expect(page.locator('[data-clip-state="ready"]')).toHaveCount(1, { timeout: 60_000 });

  const [token] = await readyClips(page);
  const { wide, vertical } = clipFiles(token);
  const w = probe(wide);
  const v = probe(vertical);
  expect([w.width, w.height]).toEqual([640, 360]);
  expect([v.width, v.height]).toEqual([360, 640]);
  for (const p of [w, v]) {
    expect(p.seconds).toBeGreaterThan(28.5);
    expect(p.seconds).toBeLessThan(31.5);
    expect(p.streams, 'no audio track although the camera sends audio').toEqual(['video']);
  }
  // Logo top-right (default corner): see media.rs place() — wide 116×46 at 11 px, vertical 130×52 at 11 px.
  expect(magenta(colour(wide, 40, 20, 551, 24)), 'logo burned into the wide clip').toBe(true);
  expect(magenta(colour(vertical, 40, 20, 264, 27)), 'logo burned into the vertical clip').toBe(true);
  expect(magenta(colour(wide, 40, 20, 40, 24)), 'other corners stay clear').toBe(false);
});

for (const [label, engine, device] of [
  ['Android', chromium, devices['Pixel 7']],
  ['iPhone', webkit, devices['iPhone 14']],
] as const) {
  test(`a ready clip downloads as MP4 in both shapes on ${label} (FR-19)`, async () => {
    const browser: Browser = await engine.launch();
    try {
      const phone = await (await browser.newContext({ ...device, acceptDownloads: true })).newPage();
      await phone.goto(`${PLAYER}/c/${code}`);
      await phone.locator('[data-clip-state="ready"] a.clip-link').first().click();
      await expect(phone.getByRole('heading', { name: /Clip from/ })).toBeVisible();
      const token = phone.url().split('/').pop()!;
      for (const [name, file] of [['Download', 'wide'], ['Download vertical 9:16', 'vertical']] as const) {
        const [download] = await Promise.all([phone.waitForEvent('download'), phone.getByRole('link', { name, exact: true }).click()]);
        expect(download.suggestedFilename()).toMatch(new RegExp(`^court-3-\\d{8}-\\d{6}-${file}\\.mp4$`));
        const saved = await download.path();
        const fs = await import('node:fs');
        const size = fs.statSync(saved).size;
        expect(size).toBe(Number(boxExec(`stat -c %s /data/clips/${token}/${file}.mp4`).trim()));
        expect(fs.readFileSync(saved).subarray(4, 8).toString()).toBe('ftyp');
      }
    } finally {
      await browser.close();
    }
  });
}

test('three taps within 5 s make exactly one clip that ends 5 s after the last tap (FR-5)', async ({ page }) => {
  await page.goto(`${PLAYER}/c/${code}`);
  const before = (await readyClips(page)).length;
  const tapButton = page.getByRole('button', { name: 'Clip that' });
  await tapButton.click();
  await page.waitForTimeout(2000);
  await tapButton.click();
  await page.waitForTimeout(2000);
  await tapButton.click();
  await expect(page.locator('[data-clip-state="saving"]')).toHaveCount(1);
  await expect(page.locator('[data-clip-state="ready"]')).toHaveCount(before + 1, { timeout: 60_000 });
  const [token] = await readyClips(page);
  const seconds = probe(clipFiles(token).wide).seconds;
  expect(seconds).toBeGreaterThan(32.5); // 25 s before the first tap + 4 s between taps + 5 s after the last
  expect(seconds).toBeLessThan(35.5);
});

test('clips are reachable only from their own court page, within 2 h, by their own token (FR-18, SN-1)', async ({ page, request }) => {
  const [token] = await readyClips(page);
  const get = async (url: string): Promise<number> => (await request.get(url)).status();
  expect(await get(`${PLAYER}/c/${code}/clips/${token}`)).toBe(200);
  expect(await get(`${PLAYER}/c/${otherCode}/clips/${token}`)).toBe(404);
  expect(await get(`${PLAYER}/c/${otherCode}/clips/${token}/wide.mp4`)).toBe(404);
  expect(await get(`${PLAYER}/c/${code}/clips/${'0'.repeat(64)}`)).toBe(404);
  expect(await get(`${PLAYER}/c/${code}/clips/../../console`)).toBe(404);
  const hideElsewhere = await request.post(`${PLAYER}/c/${otherCode}/clips/${token}/hide`, { headers: { Origin: PLAYER } });
  expect(hideElsewhere.status()).toBe(404);
  const crossSite = await request.post(`${PLAYER}/c/${code}/clips/${token}/hide`, { headers: { Origin: 'http://evil.example' } });
  expect(crossSite.status()).toBe(403);
  expect(await readyClips(page)).toContain(token);

  await advanceClock(request, 2 * 3600_000 + 60_000);
  expect(await get(`${PLAYER}/c/${code}/clips/${token}`)).toBe(404);
  expect(await get(`${PLAYER}/c/${code}/clips/${token}/wide.mp4`)).toBe(404);
  await page.goto(`${PLAYER}/c/${code}`);
  await expect(page.locator('[data-clip-state]')).toHaveCount(0);
  await expect(page.getByText('No clips from the last 2 hours')).toBeVisible();
});

test('a player hides a clip from the court page; it is gone from the page (FR-9)', async ({ page }) => {
  await page.goto(`${PLAYER}/c/${code}`);
  await page.getByRole('button', { name: 'Clip that' }).click();
  await expect(page.locator('[data-clip-state="ready"]')).toHaveCount(1, { timeout: 60_000 });
  await page.locator('[data-clip-state="ready"] a.clip-link').click();
  const token = page.url().split('/').pop()!;
  await page.getByRole('button', { name: 'Hide from this page' }).click();
  await expect(page).toHaveURL(`${PLAYER}/c/${code}`);
  await expect(page.locator('[data-clip-state]')).toHaveCount(0);
  expect((await page.request.get(`${PLAYER}/c/${code}/clips/${token}`)).status()).toBe(404);
});

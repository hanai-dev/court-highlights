import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import path from 'node:path';
import { expect, type APIRequestContext, type Page } from '@playwright/test';

const here = path.dirname(fileURLToPath(import.meta.url));
export const COMPOSE_FILE = path.resolve(here, '../../../docker/compose.test.yaml');
export const PLAYER = 'http://localhost:18080';
export const STAFF = 'https://localhost:18443';

export interface Owner {
  venue: string;
  name: string;
  username: string;
  password: string;
}

export const OWNER: Owner = { venue: 'Paddle Point Test Club', name: 'Olive Owner', username: 'olive', password: 'court-box-owner-pass-1' };

export function compose(...args: string[]): string {
  return execFileSync('docker', ['compose', '-f', COMPOSE_FILE, ...args], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
}

async function waitHealthy(url: string): Promise<void> {
  for (let i = 0; i < 60; i++) {
    try {
      if ((await fetch(url)).ok) return;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`box did not become healthy: ${url}`);
}

/** A newly installed box: empty data volume, first start. */
export async function freshBox(): Promise<void> {
  compose('down', '-v', '--remove-orphans');
  compose('up', '-d', 'box-a');
  await waitHealthy(`${PLAYER}/healthz`);
}

export const CAMERA = (n: number): string => `rtsp://rtsp:8554/court${n}`;

/** Fake cameras: MediaMTX plus one publisher per court (cam-1 also sends audio). */
export async function startCameras(...cams: string[]): Promise<void> {
  compose('up', '-d', 'rtsp', ...(cams.length ? cams : ['cam-1', 'cam-2']));
  await new Promise((r) => setTimeout(r, 4000)); // publishers connect and send a first keyframe
}

/** Restart the box process without touching its data (persistence checks). */
export async function restartBox(): Promise<void> {
  compose('restart', 'box-a');
  await waitHealthy(`${PLAYER}/healthz`);
}

export function boxExec(command: string): string {
  return compose('exec', '-T', 'box-a', 'sh', '-c', command);
}

/** Opening hours default to the whole day so tests pass at any real time of day. */
export async function completeSetup(page: Page, owner: Owner = OWNER, hours: [string, string] = ['00:00', '23:59'], logo?: string): Promise<void> {
  await page.goto(`${STAFF}/setup`);
  await page.getByLabel('Venue name').fill(owner.venue);
  if (logo) await page.getByLabel(/Venue logo/).setInputFiles(logo);
  await page.getByLabel('Opens').fill(hours[0]);
  await page.getByLabel('Closes').fill(hours[1]);
  await page.getByLabel('Your name').fill(owner.name);
  await page.getByLabel('Owner username').fill(owner.username);
  await page.getByLabel('Owner password').fill(owner.password);
  await page.getByRole('button', { name: 'Continue' }).click();
  await expect(page).toHaveURL(`${STAFF}/console`);
}

export async function signIn(page: Page, username: string, password: string): Promise<void> {
  await page.goto(`${STAFF}/console/signin`);
  await page.getByLabel('Username').fill(username);
  await page.getByLabel('Password').fill(password);
  await page.getByRole('button', { name: 'Sign in' }).click();
}

/** Test builds only (`--features test-hooks`): move the box clock forward; returns the box's new "now". */
export async function advanceClock(request: APIRequestContext, ms: number): Promise<Date> {
  const res = await request.post(`${STAFF}/__test/clock`, { form: { ms: String(ms) } });
  expect(res.ok()).toBeTruthy();
  return new Date(await res.text());
}

/** Milliseconds from the box's "now" until the next hh:mm:ss in Manila (UTC+8). */
export async function msUntilManila(request: APIRequestContext, hh: number, mm: number, ss = 0): Promise<number> {
  const now = await advanceClock(request, 0);
  const manilaNow = new Date(now.getTime() + 8 * 3600_000);
  const target = new Date(Date.UTC(manilaNow.getUTCFullYear(), manilaNow.getUTCMonth(), manilaNow.getUTCDate(), hh, mm, ss));
  if (target <= manilaNow) target.setUTCDate(target.getUTCDate() + 1);
  return target.getTime() - manilaNow.getTime();
}

export async function addCourt(page: Page, name: string, cameraUrl: string): Promise<void> {
  await page.goto(`${STAFF}/console/courts/new`);
  await page.getByLabel('Court name').fill(name);
  await page.getByLabel('Camera stream address').fill(cameraUrl);
  await page.getByRole('button', { name: 'Save court' }).click();
  await expect(page.getByRole('status')).toContainText('Saved');
}

/** The court code shown on the console home. */
export async function courtCode(page: Page, court: string): Promise<string> {
  await page.goto(`${STAFF}/console`);
  return ((await page.locator(`tr[data-court="${court}"] td.num`).textContent()) ?? '').trim();
}

/** Polls the console home until the court's status text matches. */
export async function waitCourtStatus(page: Page, court: string, pattern: RegExp, timeoutMs = 20_000): Promise<number> {
  const started = Date.now();
  await expect
    .poll(async () => {
      await page.goto(`${STAFF}/console`);
      return (await page.locator(`tr[data-court="${court}"] [data-court-status]`).textContent()) ?? '';
    }, { timeout: timeoutMs, intervals: [500] })
    .toMatch(pattern);
  return Date.now() - started;
}

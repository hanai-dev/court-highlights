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

/** Restart the box process without touching its data (persistence checks). */
export async function restartBox(): Promise<void> {
  compose('restart', 'box-a');
  await waitHealthy(`${PLAYER}/healthz`);
}

export function boxExec(command: string): string {
  return compose('exec', '-T', 'box-a', 'sh', '-c', command);
}

export async function completeSetup(page: Page, owner: Owner = OWNER): Promise<void> {
  await page.goto(`${STAFF}/setup`);
  await page.getByLabel('Venue name').fill(owner.venue);
  await page.getByLabel('Opens').fill('06:00');
  await page.getByLabel('Closes').fill('23:00');
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

/** Test builds only (`--features test-hooks`): move the box clock forward. */
export async function advanceClock(request: APIRequestContext, ms: number): Promise<void> {
  const res = await request.post(`${STAFF}/__test/clock`, { form: { ms: String(ms) } });
  expect(res.ok()).toBeTruthy();
}

import { test, expect, type Page } from '@playwright/test';
import { CLOUD, cloudSql, cloudUp, compose, createAdmin, fleetSignIn, registerBox } from './helpers';

test.describe.configure({ mode: 'serial' });

const READY = { Authorization: 'Bearer ch-test-only-ready-token' };
const readyz = async (): Promise<[number, string]> => {
  const res = await fetch(`${CLOUD}/readyz`, { headers: READY });
  return [res.status, await res.text()];
};

/** What a box does for one clip: register it, upload it, mark it done, have it emailed. */
async function sendVideo(page: Page, key: string, ref: string, recipient: string): Promise<void> {
  const api = { Authorization: `Bearer ${key}` };
  const created = await page.request.post(`${CLOUD}/api/v1/items`, {
    headers: api,
    data: { ref, kind: 'tap', court: 'Court P', made_at: new Date().toISOString(), expires_at: new Date(Date.now() + 3 * 86400_000).toISOString() },
  });
  expect(created.status()).toBe(200);
  const { id, upload_url } = (await created.json()) as { id: number; upload_url: string };
  // Signed for the store's address on the service network, so upload from there.
  compose('exec', '-T', 'cloud', 'sh', '-c', `printf 'ftypmp42-test' > /tmp/p.mp4 && curl -fsS -X PUT -H 'content-type: video/mp4' --data-binary @/tmp/p.mp4 '${upload_url}'`);
  expect((await page.request.post(`${CLOUD}/api/v1/items/${id}/done`, { headers: api })).status()).toBe(204);
  const sent = await page.request.post(`${CLOUD}/api/v1/deliveries`, { headers: api, data: { ref: `${ref}-d`, title: 'Game', items: [id], recipients: [recipient] } });
  expect(sent.ok()).toBe(true);
}

test('liveness is open and says nothing; readiness needs the platform token; a database or storage outage shows not ready without saying which (HOST-health)', async () => {
  test.setTimeout(240_000);
  await cloudUp();
  const live = await fetch(`${CLOUD}/healthz`);
  expect([live.status, await live.text()]).toEqual([200, 'ok']);
  expect((await fetch(`${CLOUD}/readyz`)).status, 'readiness without the token').toBe(401);
  expect(await readyz()).toEqual([200, 'ready']);

  for (const part of ['db', 's3']) {
    compose('stop', part);
    await expect.poll(readyz, { timeout: 60_000 }).toEqual([503, 'not ready']);
    expect((await fetch(`${CLOUD}/healthz`)).status, `still alive without ${part}`).toBe(200);
    compose('start', part);
    await expect.poll(readyz, { timeout: 90_000, intervals: [2000] }).toEqual([200, 'ready']);
  }
});

test('fleet accounts, registrations, sent items and blocked addresses survive a restart and a redeploy (HOST-persistence)', async ({ page }) => {
  test.setTimeout(300_000);
  const admin = createAdmin('persist');
  await sendVideo(page, registerBox('Persistence Club'), 'persist', 'pia@example.com');
  cloudSql("INSERT INTO blocked (email_hash) VALUES (repeat('ab', 32)) ON CONFLICT DO NOTHING");
  await fleetSignIn(page, admin); // a fleet log entry
  await expect(page).toHaveURL(`${CLOUD}/fleet/account`);

  const counts = (): string => cloudSql("SELECT concat_ws(',', (SELECT count(*) FROM fleet_user), (SELECT count(*) FROM box), (SELECT count(*) FROM item), (SELECT count(*) FROM delivery), (SELECT count(*) FROM delivery_item), (SELECT count(*) FROM blocked), (SELECT count(*) FROM fleet_log))");
  const before = counts();
  expect(before.split(',').every((n) => Number(n) > 0), before).toBe(true);

  compose('restart', 'cloud');
  await cloudUp();
  expect(counts(), 'after a restart').toBe(before);
  compose('up', '-d', '--force-recreate', '--no-deps', 'cloud'); // a redeploy: a new container, same database
  await cloudUp();
  expect(counts(), 'after a redeploy').toBe(before);

  // The account still works after both.
  await fleetSignIn(page, admin);
  await expect(page).toHaveURL(`${CLOUD}/fleet/account`);
});

test('metrics count uploads, emails, check-ins and fleet actions; the logs are structured and carry no addresses, keys or tokens (HOST-observability)', async ({ page }) => {
  test.setTimeout(180_000);
  const key = registerBox('Observed Club');
  const checkin = await page.request.post(`${CLOUD}/api/v1/checkin`, {
    headers: { Authorization: `Bearer ${key}` },
    data: { version: '0.1.0', disk_pct: 12, courts_recording: 1, courts_total: 1 },
  });
  expect(checkin.ok()).toBe(true);
  await sendVideo(page, key, 'observed', 'quinn@example.com');
  const admin = createAdmin('observer');
  await fleetSignIn(page, admin);
  await expect(page).toHaveURL(`${CLOUD}/fleet/account`);

  expect((await fetch(`${CLOUD}/metrics`)).status, 'metrics need the platform token').toBe(401);
  const text = await (await fetch(`${CLOUD}/metrics`, { headers: READY })).text();
  for (const name of ['ch_uploads_total', 'ch_emails_sent_total', 'ch_checkins_total', 'ch_fleet_actions_total']) {
    expect(Number(text.match(new RegExp(`^${name} (\\d+)$`, 'm'))?.[1] ?? 0), name).toBeGreaterThan(0);
  }
  expect(text).toMatch(/^ch_emails_failed_total \d+$/m);

  const lines = compose('logs', '--no-color', '--no-log-prefix', 'cloud').trim().split('\n').filter((l) => l.startsWith('{'));
  const events = lines.map((l) => JSON.parse(l) as { level: string; fields: { message: string } });
  for (const message of ['item uploaded', 'take-home email', 'check-in', 'fleet action']) {
    expect(events.some((e) => e.fields.message === message), message).toBe(true);
  }
  const all = lines.join('\n');
  expect(all).not.toMatch(/[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/);
  expect(all).not.toContain(key);
  expect(all).not.toContain('ch-test-only-ready-token');
  expect(all).not.toMatch(/\b[0-9a-f]{64}\b/);
});

test('the service refuses to start with missing or malformed configuration and never prints a secret value (HOST-configuration)', () => {
  const start = (env: Record<string, string>): string => {
    const args = ['run', '--rm', '--no-deps', ...Object.entries(env).flatMap(([k, v]) => ['-e', `${k}=${v}`]), 'cloud', './target/debug/ch-cloud'];
    try {
      compose(...args);
      return 'started';
    } catch (e) {
      const err = e as { status: number; stderr: string; stdout: string };
      expect(err.status, JSON.stringify(env)).not.toBe(0);
      return `${err.stdout}\n${err.stderr}`;
    }
  };
  const cases: [Record<string, string>, RegExp][] = [
    [{ CH_SES_KEY: '' }, /CH_SES_KEY is not set/],
    [{ DATABASE_URL: '' }, /DATABASE_URL is not set/],
    [{ CH_EMAIL_PEPPER: 'short' }, /CH_EMAIL_PEPPER must be at least 32 characters/],
    [{ CH_SECRET_KEY: 'not-hex' }, /CH_SECRET_KEY/],
    [{ CH_S3_SECRET: 'wrong-secret-value' }, /object store unreachable/],
  ];
  for (const [env, expected] of cases) {
    const out = start(env);
    expect(out, JSON.stringify(env)).toMatch(expected);
    for (const secret of ['ch-test-only-not-a-secret', 'ch-test-only-pepper-not-a-secret', 'wrong-secret-value', '0000000000000000000000000000000000000000000000000000000000000001']) {
      expect(out, `no secret value in the output for ${JSON.stringify(env)}`).not.toContain(secret);
    }
  }
});

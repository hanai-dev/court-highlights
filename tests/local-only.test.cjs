// The system works on the venue network only (owner decision 2026-10-22): no online service, no internet calls, no
// player email, no fleet console. This test fails while any of it is left, area by area.
//   node --test tests/local-only.test.cjs                 the regression test (CI)
//   node tests/local-only.test.cjs --count                hits per area, for the fix loop's metric
// A line may name what was removed when it carries the marker `local-only:` (a change record, a test that checks absence).
const test = require('node:test');
const assert = require('node:assert');
const fs = require('node:fs');
const path = require('node:path');

const ROOT = path.resolve(__dirname, '..');
const SKIP_DIRS = new Set(['.git', 'node_modules', 'target', 'test-results', 'playwright-report']);

function files(dir, keep) {
  const out = [];
  const walk = (d) => {
    if (!fs.existsSync(d)) return;
    for (const e of fs.readdirSync(d, { withFileTypes: true })) {
      const p = path.join(d, e.name);
      if (e.isDirectory()) {
        if (!SKIP_DIRS.has(e.name)) walk(p);
      } else if (keep(p)) out.push(p);
    }
  };
  walk(path.join(ROOT, dir));
  return out;
}

function hits(paths, pattern) {
  const found = [];
  for (const file of paths) {
    if (path.resolve(file) === __filename) continue;
    fs.readFileSync(file, 'utf8').split('\n').forEach((line, i) => {
      if (pattern.test(line) && !line.includes('local-only:')) found.push(`${path.relative(ROOT, file)}:${i + 1}: ${line.trim().slice(0, 120)}`);
    });
  }
  return found;
}

const code = (p) => /\.(rs|html|js|css|toml)$/.test(p);
const AREAS = {
  // The online service itself and everything that builds, runs or tests it.
  'online service': () => [
    ...['crates/cloud', 'docker/cloud.Dockerfile', 'tests/fixtures/fake-ses.cjs', 'tools/test/s3.json', 'tools/cloud-image-check.sh',
      'tools/cost-model.cjs', 'tools/migration-check.cjs', 'tests/e2e/specs/cloud-ops.spec.ts', 'tests/e2e/specs/fleet.spec.ts',
      'tests/e2e/specs/online.spec.ts']
      .filter((p) => fs.existsSync(path.join(ROOT, p)))
      .map((p) => `${p}: exists`),
    ...hits(
      [path.join(ROOT, 'Cargo.toml'), path.join(ROOT, 'docker/compose.test.yaml'), path.join(ROOT, '.github/workflows/ci.yml'),
        ...files('install', () => true), ...files('tools', (p) => /\.(cjs|sh|js)$/.test(p))],
      /ch-cloud|crates\/cloud|cloud\.Dockerfile|cloud-image|^\s*(cloud|db|s3|ses):|seaweedfs|fake-ses|CH_SES|CH_S3|DATABASE_URL|postgres|CH_CLOUD_URL|debian 13/i,
    ),
  ],
  // The box calling out: check-ins, sending, the box key, the upload cap, the online switch, the send order.
  'box uplink': () =>
    hits([...files('crates/box/src', code), ...files('crates/box/templates', code), ...files('crates/core', code)],
      /uplink|CH_CLOUD_URL|CH_CHECKIN|cloud|check-?in|online_on|online features|upload[_ ]cap|cap_kbps|send_order|Outgoing|\/api\/v1\/|withdraw/i),
  // Player email: the address list, "Email me this", the email-sized copies.
  'player email': () => hits([...files('crates/box', code), ...files('crates/core', code)], /e-?mail/i),
  // The fleet console's reach into the box: fleet updates, leaving the fleet.
  fleet: () => hits([...files('crates/box', code), ...files('crates/core', code)], /fleet/i),
  // The end-to-end suite: no stand-ins for the online service, no mail checks.
  'e2e suite': () =>
    hits(files('tests/e2e/specs', (p) => p.endsWith('.ts')).concat(files('tests/e2e', (p) => /global-teardown\.ts$|playwright\.config\.ts$/.test(p))),
      /cloudUp|CLOUD\b|sentMail|clearMail|registerBox|connectBox|createAdmin|fleetSignIn|cloudSql|18090|18025|e-?mail|fleet/i),
  // What people read: the product documents describe a venue-network system.
  documents: () =>
    hits(['README.md', 'RUNBOOK.md', 'RELEASE_NOTES.md', 'DEBT.md', 'DESIGN.md', 'evals-summary.md', 'docs/user-manual.md', 'docs/HLD.md', 'docs/charter.md']
      .map((p) => path.join(ROOT, p)).filter((p) => fs.existsSync(p)),
      /online service|fleet|e-?mail|take-home|\bSES\b|\bR2\b|DigitalOcean|PostgreSQL|check-?in|upload cap|box key/i),
};

if (process.argv.includes('--count')) {
  let total = 0;
  for (const [area, find] of Object.entries(AREAS)) {
    const n = find().length;
    total += n;
    console.log(`${String(n).padStart(5)}  ${area}`);
  }
  console.log(`LOCAL-ONLY: ${total} cloud reference(s) left`);
  process.exit(0);
}

for (const [area, find] of Object.entries(AREAS)) {
  test(`local only: ${area}`, () => {
    const found = find();
    assert.deepStrictEqual(found, [], `${found.length} left:\n${found.slice(0, 40).join('\n')}`);
  });
}

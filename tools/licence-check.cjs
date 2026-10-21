// Licence gate (A-114, NFR-10): every crate in the resolved dependency graph must carry a permissive licence.
// GPL/AGPL anywhere fails; LGPL is allowed only for separately-linked system libraries, never for crates.
// Usage: node tools/licence-check.cjs [metadata.json]   (exit 1 on a violation; prints a per-licence tally)
// With no file it runs `cargo metadata`; pass a saved `cargo metadata --format-version 1 --locked` where node and
// cargo live in different places (the build image has no node).
const { execFileSync } = require('child_process');
const fs = require('fs');

const meta = JSON.parse(
  process.argv[2]
    ? fs.readFileSync(process.argv[2], 'utf8')
    : execFileSync('cargo', ['metadata', '--format-version', '1', '--locked'], { maxBuffer: 64 * 1024 * 1024 }),
);
const ours = new Set(meta.workspace_members);
const bad = [];
const tally = {};
for (const p of meta.packages) {
  if (ours.has(p.id)) continue;
  const lic = (p.license || '(none)').replace(/\//g, ' OR ');
  tally[lic] = (tally[lic] || 0) + 1;
  // An SPDX "OR" expression is fine when at least one alternative is permissive.
  const alternatives = lic.split(/\s+OR\s+/i);
  if (alternatives.every((a) => /GPL|\(none\)/i.test(a))) bad.push(`${p.name}@${p.version}: ${lic}`);
}
for (const [lic, n] of Object.entries(tally).sort((a, b) => b[1] - a[1])) console.log(`${String(n).padStart(4)}  ${lic}`);
if (bad.length) {
  console.log(`LICENCE: FAIL\n${bad.join('\n')}`);
  process.exit(1);
}
console.log(`LICENCE: PASS (${meta.packages.length - ours.size} crates, no GPL/AGPL-only or unlicensed crate)`);

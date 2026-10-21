// Log privacy gate (SEC-LOGS-NO-PII, NFR-15): box and online-service logs saved during a full e2e run must not hold
// a player email, a password, a session or link token, or a key.
// Usage: node tools/log-pii-scan.cjs <saved logs> [known secret …]   (exit 1 on a finding)
// Known secrets are the test run's own values (passwords, keys) that must never appear.
const fs = require('fs');

const [file, ...known] = process.argv.slice(2);
if (!file) {
  console.error('usage: node tools/log-pii-scan.cjs <saved logs> [known secret …]');
  process.exit(2);
}
const lines = fs.readFileSync(file, 'utf8').split('\n');
const rules = [
  ['email address', /[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}/],
  ['64-hex token or key', /\b[0-9a-f]{64}\b/i],
  ['bearer credential', /Bearer\s+\S{16,}/],
  ['password hash', /\$argon2id\$/],
  ['password field', /"?password"?\s*[:=]\s*"?[^\s",}]{4,}/i],
  ...known.filter((k) => k.length >= 6).map((k) => [`known secret ${k.slice(0, 3)}…`, new RegExp(k.replace(/[.*+?^${}()|[\]\\]/g, '\\$&'))]),
];
const findings = [];
for (const [i, line] of lines.entries()) {
  for (const [name, re] of rules) if (re.test(line)) findings.push(`${name} at line ${i + 1}: ${line.slice(0, 160).replace(re, '«redacted»')}`);
}
const services = new Set(lines.map((l) => l.split(/\s+\|/)[0].trim()).filter((s) => s && !s.startsWith('{')));
console.log(`scanned ${lines.length} log lines from ${[...services].sort().join(', ') || 'unknown services'}; rules: ${rules.length}`);
if (findings.length) {
  console.log(findings.slice(0, 50).join('\n'));
  console.log(`LOG-PII: FAIL (${findings.length} finding${findings.length === 1 ? '' : 's'})`);
  process.exit(1);
}
console.log('LOG-PII: PASS (no addresses, tokens, keys, password hashes or known secrets)');

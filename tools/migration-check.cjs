// Schema change gate (HOST-compatibility): every online-service migration only adds — new tables, indexes, and columns
// that are nullable or have a default — so the previous release keeps working on the migrated database and rolling back
// is redeploying the previous image. Anything that removes, renames or tightens fails.
// Usage: node tools/migration-check.cjs [migrations dir]   (exit 1 on a breaking statement)
const fs = require('fs');
const path = require('path');

const dir = process.argv[2] || path.join(__dirname, '..', 'crates', 'cloud', 'migrations');
const files = fs.readdirSync(dir).filter((f) => f.endsWith('.sql')).sort();
const breaking = [
  [/\bDROP\s+(TABLE|COLUMN|INDEX|CONSTRAINT|TYPE|VIEW)\b/i, 'drops'],
  [/\bRENAME\b/i, 'renames'],
  [/\bALTER\s+COLUMN\b[^;]*\b(TYPE|SET\s+NOT\s+NULL)\b/i, 'changes a column type or tightens it'],
  [/\bADD\s+COLUMN\b(?![^;]*\bDEFAULT\b)[^;]*\bNOT\s+NULL\b/i, 'adds a NOT NULL column without a default'],
  [/\bADD\s+(CONSTRAINT\b[^;]*\b)?(CHECK|UNIQUE|FOREIGN\s+KEY)\b/i, 'adds a constraint existing rows may break'],
  [/\b(DELETE\s+FROM|TRUNCATE|UPDATE\s+\w+\s+SET)\b/i, 'rewrites existing data'],
];
const findings = [];
// The first migration creates the schema; the rule applies from the second release on.
for (const f of files.slice(1)) {
  const sql = fs.readFileSync(path.join(dir, f), 'utf8').replace(/--[^\n]*/g, '');
  for (const statement of sql.split(';').map((s) => s.trim()).filter(Boolean)) {
    for (const [re, why] of breaking) if (re.test(statement)) findings.push(`${f}: ${why}: ${statement.replace(/\s+/g, ' ').slice(0, 120)}`);
  }
}
console.log(`checked ${files.length} migrations (${files.join(', ')})`);
if (findings.length) {
  console.log(findings.join('\n'));
  console.log('MIGRATIONS: FAIL (a breaking change needs an expand/contract plan across two releases)');
  process.exit(1);
}
console.log('MIGRATIONS: PASS (additive only: the previous release runs on the migrated schema)');

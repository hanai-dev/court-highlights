// DEV-PAYLOAD: every shipped UI asset (stylesheet, scripts, drawings) stays within the planning budget in
// docs/requirements/assets/manifest.json. Usage: node tools/payload-check.cjs  → prints PAYLOAD: PASS|FAIL.
const fs = require('node:fs');
const path = require('node:path');

const root = path.resolve(__dirname, '..');
const manifest = JSON.parse(fs.readFileSync(path.join(root, 'docs/requirements/assets/manifest.json'), 'utf8'));
const { maxFileBytes, maxTotalBytes } = manifest.budget;
const dir = path.join(root, 'crates/box/static');

let total = 0;
let ok = true;
const walk = (d) => fs.readdirSync(d, { withFileTypes: true }).flatMap((e) => (e.isDirectory() ? walk(path.join(d, e.name)) : [path.join(d, e.name)]));
for (const file of walk(dir).sort()) {
  const name = path.relative(dir, file).split(path.sep).join('/');
  const bytes = fs.statSync(file).size;
  total += bytes;
  const over = bytes > maxFileBytes;
  ok &&= !over;
  console.log(`${over ? 'OVER' : 'ok  '} ${String(bytes).padStart(7)} B  ${name}`);
}
const overTotal = total > maxTotalBytes;
ok &&= !overTotal;
console.log(`${overTotal ? 'OVER' : 'ok  '} ${String(total).padStart(7)} B  total (budget ${maxTotalBytes} B, per file ${maxFileBytes} B)`);
console.log(`PAYLOAD: ${ok ? 'PASS' : 'FAIL'}`);
process.exit(ok ? 0 : 1);

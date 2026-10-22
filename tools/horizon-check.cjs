// DEV-HORIZON release checklist: every pinned OS, database and runtime stays supported through the 3-year horizon
// (2030-06-30, A-116), checked against endoflife.date. Components without an end-of-life calendar are listed with
// the reason they are covered. Usage: node tools/horizon-check.cjs → prints HORIZON: PASS|FAIL.
const HORIZON = '2030-06-30';

const CHECKED = [
  { what: 'Box OS: Ubuntu 26.04 LTS', product: 'ubuntu', cycle: '26.04', field: 'eol' },
  // Free LTS ends exactly on the horizon (paid extended support runs to 2035); rebase the image before then.
];

const COVERED = [
  ['GStreamer 1.28, OpenH264, intel-media-va-driver', 'shipped as Ubuntu 26.04 LTS packages: security-supported with the OS'],
  ['Rust 1.99 toolchain and crates', 'build-time only; every box release ships a freshly built binary'],
  ['ONNX Runtime via ort 2.0.0-rc.15 (pinned exactly)', 'bundled with the box release; replaced in a box update, no OS dependency'],
];

async function main() {
  let ok = true;
  for (const c of CHECKED) {
    const res = await fetch(`https://endoflife.date/api/${c.product}.json`);
    const cycles = await res.json();
    const row = cycles.find((r) => String(r.cycle) === c.cycle);
    const end = row && (typeof row[c.field] === 'string' ? row[c.field] : row.eol);
    const pass = typeof end === 'string' && end >= HORIZON;
    ok &&= pass;
    console.log(`${pass ? 'ok  ' : 'FAIL'} ${c.what.padEnd(52)} supported until ${end ?? 'unknown'}`);
  }
  for (const [what, why] of COVERED) console.log(`ok   ${what.padEnd(52)} ${why}`);
  console.log(`HORIZON: ${ok ? 'PASS' : 'FAIL'} (through ${HORIZON})`);
  process.exitCode = ok ? 0 : 1;
}

main().catch((e) => {
  console.log(`HORIZON: FAIL (${e.message})`);
  process.exitCode = 1;
});

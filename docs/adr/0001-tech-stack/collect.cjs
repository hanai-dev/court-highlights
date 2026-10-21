// Evidence sweep for the stack decision: registries, deps.dev, OSV, endoflife.date, GitHub repo metadata.
// Writes raw JSON to stack/evidence/raw/ and a compact summary to stack/evidence/registry-summary.md.
// Usage: node stack/collect.cjs
const fs = require('fs');
const path = require('path');

const RAW = path.join(__dirname, 'evidence', 'raw');
fs.mkdirSync(RAW, { recursive: true });
const UA = { 'User-Agent': 'forge-requirements-stack-sweep (jessiondiwangan@gmail.com)' };
const today = new Date().toISOString().slice(0, 10);

const CRATES = ['tokio', 'axum', 'retina', 'gstreamer', 'gstreamer-video', 'ort', 'rusqlite', 'askama',
  'argon2', 'totp-rs', 'aws-sdk-s3', 'aws-sdk-sesv2', 'sqlx', 'tower-http'];
const PYPI = ['fastapi', 'uvicorn', 'openvino', 'onnxruntime', 'peewee'];
const EOL = ['python', 'ubuntu', 'postgresql', 'rust', 'nvidia-jetson', 'jetpack', 'openvino', 'ffmpeg', 'debian'];
const REPOS = ['blakeblackshear/frigate', 'scottlamb/retina', 'pykeio/ort', 'openvinotoolkit/openvino',
  'microsoft/onnxruntime', 'FFmpeg/FFmpeg', 'GStreamer/gstreamer', 'rust-lang/rust', 'tokio-rs/axum', 'ultralytics/ultralytics'];

async function get(url, opts = {}) {
  try {
    const r = await fetch(url, { headers: UA, ...opts });
    const text = await r.text();
    let json = null; try { json = JSON.parse(text); } catch {}
    return { status: r.status, json, text };
  } catch (e) { return { status: 0, error: e.message }; }
}
const save = (name, obj) => fs.writeFileSync(path.join(RAW, name), JSON.stringify(obj, null, 2));
const lines = [`# Registry / advisory / EOL / repo sweep — accessed ${today}`, ''];

(async () => {
  // crates.io + deps.dev
  lines.push('## crates.io + deps.dev (cargo)', '', '| crate | latest stable | released | licence (crates.io) | deps.dev licences | advisories (deps.dev) | repo |', '|---|---|---|---|---|---|---|');
  const osvQueries = [];
  for (const c of CRATES) {
    const cr = await get(`https://crates.io/api/v1/crates/${c}`);
    save(`crates-${c}.json`, cr.json || cr);
    const k = cr.json && cr.json.crate;
    const ver = k ? (k.max_stable_version || k.max_version) : '?';
    const vrow = cr.json && cr.json.versions ? cr.json.versions.find((v) => v.num === ver) : null;
    const dd = await get(`https://api.deps.dev/v3/systems/cargo/packages/${encodeURIComponent(c)}/versions/${encodeURIComponent(ver)}`);
    save(`depsdev-cargo-${c}.json`, dd.json || dd);
    const lic = dd.json && dd.json.licenses ? dd.json.licenses.join(' / ') : `status ${dd.status}`;
    const adv = dd.json && dd.json.advisoryKeys ? dd.json.advisoryKeys.length : '?';
    lines.push(`| ${c} | ${ver} | ${vrow ? vrow.created_at.slice(0, 10) : '?'} | ${vrow ? vrow.license : '?'} | ${lic} | ${adv} | ${k ? k.repository : '?'} |`);
    osvQueries.push({ package: { name: c, ecosystem: 'crates.io' }, version: ver });
  }
  // PyPI + deps.dev
  lines.push('', '## PyPI + deps.dev (pypi)', '', '| package | latest | requires_python | licence (PyPI) | deps.dev licences | advisories |', '|---|---|---|---|---|---|');
  for (const p of PYPI) {
    const py = await get(`https://pypi.org/pypi/${p}/json`);
    save(`pypi-${p}.json`, py.json ? { info: py.json.info, releases_count: Object.keys(py.json.releases || {}).length } : py);
    const info = py.json && py.json.info;
    const ver = info ? info.version : '?';
    const dd = await get(`https://api.deps.dev/v3/systems/pypi/packages/${p}/versions/${ver}`);
    save(`depsdev-pypi-${p}.json`, dd.json || dd);
    const lic = info ? (info.license_expression || (info.license || '').slice(0, 40) || '(classifier only)') : '?';
    lines.push(`| ${p} | ${ver} | ${info ? info.requires_python : '?'} | ${lic} | ${dd.json && dd.json.licenses ? dd.json.licenses.join(' / ') : dd.status} | ${dd.json && dd.json.advisoryKeys ? dd.json.advisoryKeys.length : '?'} |`);
    osvQueries.push({ package: { name: p, ecosystem: 'PyPI' }, version: ver });
  }
  // OSV
  const osv = await get('https://api.osv.dev/v1/querybatch', { method: 'POST', headers: { ...UA, 'Content-Type': 'application/json' }, body: JSON.stringify({ queries: osvQueries }) });
  save('osv-querybatch.json', { queries: osvQueries, response: osv.json || osv });
  lines.push('', '## OSV querybatch (pinned latest versions)', '');
  (osv.json && osv.json.results || []).forEach((r, i) => {
    const q = osvQueries[i];
    lines.push(`- ${q.package.ecosystem} ${q.package.name}@${q.version}: ${r.vulns ? r.vulns.map((v) => v.id).join(', ') : '0 vulnerabilities'}`);
  });
  // endoflife.date
  lines.push('', '## endoflife.date', '');
  for (const p of EOL) {
    const e = await get(`https://endoflife.date/api/v1/products/${p}/`);
    save(`eol-${p}.json`, e.json || { status: e.status });
    if (!e.json || !e.json.result) { lines.push(`- ${p}: HTTP ${e.status} (no product)`); continue; }
    const rel = e.json.result.releases.slice(0, 6).map((r) => `${r.name} (released ${r.releaseDate}, eoas ${r.eoasFrom || '-'}, eol ${r.eolFrom || '-'}${r.isLts ? ', LTS' : ''})`);
    lines.push(`- ${p}: ${rel.join('; ')}`);
  }
  // GitHub repo metadata
  lines.push('', '## GitHub repository metadata', '', '| repo | licence | archived | last push | stars | open issues |', '|---|---|---|---|---|---|');
  for (const r of REPOS) {
    const g = await get(`https://api.github.com/repos/${r}`);
    save(`github-${r.replace('/', '__')}.json`, g.json || g);
    const j = g.json || {};
    lines.push(`| ${r} | ${j.license ? j.license.spdx_id : '?'} | ${j.archived} | ${(j.pushed_at || '?').slice(0, 10)} | ${j.stargazers_count} | ${j.open_issues_count} |`);
  }
  fs.writeFileSync(path.join(__dirname, 'evidence', 'registry-summary.md'), lines.join('\n') + '\n');
  console.log(lines.join('\n'));
})();

// Stamps the release version into both crates and the lockfile (semantic-release prepare step, build.sh).
// The box reports CARGO_PKG_VERSION (`ch-box --version`, the update slots, the fleet), so it must be in the build;
// builds use --locked, so the lockfile must agree with the manifests.
//   node tools/release/set-version.cjs <version>
const fs = require("fs");

const version = process.argv[2] ?? "";
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(version)) throw new Error(`not a release version: "${version}"`);

const edits = [
  // [file, pattern whose first group is kept and whose quoted version is replaced, how many matches it must have]
  ["crates/box/Cargo.toml", /^(\[package\][^[]*?^version = )"[^"]*"/gm, 1],
  ["crates/core/Cargo.toml", /^(\[package\][^[]*?^version = )"[^"]*"/gm, 1],
  ["Cargo.lock", /^(name = "ch-(?:box|core)"\r?\nversion = )"[^"]*"/gm, 2],
];

for (const [file, pattern, expected] of edits) {
  const text = fs.readFileSync(file, "utf8");
  const found = [...text.matchAll(pattern)].length;
  if (found !== expected) throw new Error(`${file}: expected ${expected} version line(s), found ${found}`);
  fs.writeFileSync(file, text.replace(pattern, `$1"${version}"`));
  console.log(`${file}: ${version}`);
}

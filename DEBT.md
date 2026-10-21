# Known debt and accepted findings

Each entry says what was accepted, why, and when to look again.

## Reviewed advisories

| Advisory | Crate | Where | Decision | Recheck |
|---|---|---|---|---|
| RUSTSEC-2026-0374 (unmaintained, informational, no CVSS) | basic-toml 0.1.10 | `askama_derive`, a compile-time proc macro that reads `askama.toml` | Accepted: nothing from it is linked into `ch-box` or `ch-cloud`. | When askama drops it, or before each release (`tools/supply-chain-scan.sh`) |

## Deliberate simplifications

| What | Ceiling | Upgrade path |
|---|---|---|
| Wipe box erases files and empties the database in place (`crates/box/src/wipe.rs`) | Deleted blocks can still be read by forensic tools until the disk reuses them | On the installed image, also discard the data partition's encryption key (crypto-erase) |
| Sibling certificate pins live in one process-wide table (`crates/box/src/remote.rs`) | Fine for a handful of boxes per venue | Per-peer state on `Peer` if venues grow large |
| Camera discovery waits a fixed 1.5 s for WS-Discovery answers on each court form load | Slow cameras that answer later are missed until reload | Background discovery cache refreshed every minute |

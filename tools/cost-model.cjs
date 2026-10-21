// DEV-COST-MODEL: online running cost at the year-1 upper-bound usage (A-117) with the pinned 2026 prices
// (docs/adr/0001-tech-stack/sources.tsv S-36…S-39) must stay within the owner's budget of US$100/month (A-118).
// Usage: node tools/cost-model.cjs  → prints the breakdown and COST: PASS|FAIL.

const PRICES = {
  r2StorageGbMonth: 0.015, // S-36
  r2FreeGbMonth: 10, // S-36
  r2ClassAPerMillion: 4.5, // S-36 (PUT)
  r2ClassBPerMillion: 0.36, // S-36 (GET, HEAD)
  r2EgressPerGb: 0, // S-36: egress is free
  sesPerThousand: 0.1, // S-37
  dropletMonth: 12, // S-38: Basic 2 GiB
  managedPostgresMonth: 15.15, // S-39: 1 GiB node
};

// A-117 year-1 upper bound: ~100 courts; per court per day 6 game-hours with emails (720p) + clips ≈ 5.5 GB, 48 emails.
const USAGE = {
  courts: 100,
  gbSentPerCourtDay: 5.5,
  retentionDays: 3, // 72 h (FR-45)
  emailsPerCourtDay: 48,
  gamesPerCourtDay: 6,
  itemsPerGame: 17, // game video + reel + 5 highlights + ~10 tapped clips
  downloadsPerEmail: 3,
  days: 30,
};

const items = USAGE.courts * USAGE.gamesPerCourtDay * USAGE.itemsPerGame * USAGE.days;
const emails = USAGE.courts * USAGE.emailsPerCourtDay * USAGE.days;
const heldGb = USAGE.courts * USAGE.gbSentPerCourtDay * USAGE.retentionDays;
const lines = [
  ['R2 storage (held for 72 h)', Math.max(0, heldGb - PRICES.r2FreeGbMonth) * PRICES.r2StorageGbMonth, `${heldGb.toFixed(0)} GB`],
  ['R2 uploads (Class A)', (items / 1e6) * PRICES.r2ClassAPerMillion, `${items} PUT`],
  ['R2 checks and downloads (Class B)', ((items + emails * USAGE.downloadsPerEmail) / 1e6) * PRICES.r2ClassBPerMillion, `${items + emails * USAGE.downloadsPerEmail} GET/HEAD`],
  ['R2 egress', 0, 'free'],
  ['SES email', (emails / 1000) * PRICES.sesPerThousand, `${emails} emails`],
  ['DigitalOcean droplet (online service)', PRICES.dropletMonth, 'Basic 2 GiB'],
  ['DigitalOcean managed PostgreSQL 18', PRICES.managedPostgresMonth, '1 GiB node'],
];
const total = lines.reduce((s, [, usd]) => s + usd, 0);
for (const [what, usd, basis] of lines) console.log(`${what.padEnd(40)} US$${usd.toFixed(2).padStart(7)}  (${basis})`);
console.log(`${'Total per month'.padEnd(40)} US$${total.toFixed(2).padStart(7)}  (budget US$100, A-118)`);
console.log(`COST: ${total <= 100 ? 'PASS' : 'FAIL'}`);
process.exit(total <= 100 ? 0 : 1);

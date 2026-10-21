// Sensitivity sweep for stack-decision.md (weights locked in A-120). Usage: node stack/sweep.cjs
const W = [3, 3, 3, 3, 2, 2, 2, 2, 2, 2, 2, 1, 1];
const S = {
  'O-1': [4, 3, 2, 2, 4, 4, 4, 3, 4, 4, 5, 4, 4],
  'O-2': [4, 3, 2, 4, 4, 3, 4, 4, 4, 3, 5, 4, 4],
  'O-3': [4, 4, 5, 3, 0, 1, 4, 3, 4, 3, 4, 3, 2],
  'O-4': [5, 3, 2, 1, 4, 2, 4, 3, 4, 4, 3, 2, 4],
};
const tot = (w) => Object.fromEntries(Object.entries(S).map(([o, s]) => [o, s.reduce((a, v, i) => a + v * w[i], 0)]));
const rank = (t) => Object.entries(t).sort((a, b) => b[1] - a[1]);
const fmt = (t) => rank(t).map(([o, v]) => `${o} ${v}`).join(' · ');
const flips = (t) => { const r = rank(t); return r[0][0] !== 'O-2' || r[0][1] === r[1][1]; };

console.log('base:', fmt(tot(W)));
console.log('unweighted:', fmt(tot(W.map(() => 1))));
let held = 0, cases = 0;
for (let i = 0; i < W.length; i++) for (const d of [-1, 1]) {
  const w = [...W]; w[i] = Math.max(0, w[i] + d); cases++;
  const t = tot(w); if (flips(t)) console.log(`RQ-${i + 1} weight ${d > 0 ? '+1' : '-1'}:`, fmt(t)); else held++;
}
console.log(`±1 weight cases where O-2 leads alone: ${held}/${cases}`);
for (let i = 0; i < W.length; i++) {
  const w = [...W]; w[i] = 0; const t = tot(w);
  if (flips(t)) console.log(`drop RQ-${i + 1}:`, fmt(t));
}
const pugh = (datum) => Object.fromEntries(Object.entries(S).filter(([o]) => o !== datum).map(([o, s]) => {
  let p = 0, m = 0, wp = 0, wm = 0;
  s.forEach((v, i) => { if (v > S[datum][i]) { p++; wp += W[i]; } else if (v < S[datum][i]) { m++; wm += W[i]; } });
  return [o, `+${p}/-${m} (weighted +${wp}/-${wm})`];
}));
console.log('Pugh vs O-1:', JSON.stringify(pugh('O-1')));
console.log('Pugh vs O-2:', JSON.stringify(pugh('O-2')));

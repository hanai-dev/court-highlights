// The README figure: a 1:50 model court with its camera and court sign, beside the real court box and a phone
// showing the court page. Built with the Anatomy kit (https://github.com/wheresryan22/anatomy, MIT).
// Usage: node docs/readme/figure/build.mjs [--light]   (ANATOMY=<path to the skill> if it isn't in ~/.claude/skills)
import { readFileSync, writeFileSync } from "node:fs";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const SKILL = process.env.ANATOMY ?? join(homedir(), ".claude", "skills", "anatomy");
const k = await import(pathToFileURL(join(SKILL, "kit", "iso-kit.mjs")).href);
const { kitScript } = await import(pathToFileURL(join(SKILL, "scripts", "inline-kit.mjs")).href);

const HERE = dirname(fileURLToPath(import.meta.url));
const theme = process.argv.includes("--light") ? "light" : "dark";

// The product's truth (docs/user-manual.md): one camera per court, high and centred, 5–7 m up; a clip is the
// 25 s before the tap and 5 s after, saved in about 10 s; a second tap within 5 s makes it longer.
const BEFORE = 25;
const AFTER = 5;
const SAVE_SECS = 10;

// World units are centimetres on the bench. The court is a 1:50 model; the box and the phone are real size.
const WIDTH = 640;
const HEIGHT = 420;
const PLATE = { x: -37, y: -26, w: 46, d: 61, r: 3 };
const PLATE_H = 2.4;
const FOOT_H = 1.4;

const SURF = { x: -34, y: -8, w: 20, d: 40, r: 1 };
const SURF_H = 1.2;
const COURT = { x0: -30.1, x1: -17.9, y0: -1.4, y1: 25.4 }; // 6.10 × 13.41 m at 1:50
const NET_Y = 12;
const KITCHEN = 4.27; // 2.13 m at 1:50
const POSTS = [-30.7, -17.3];
const POST_TOP = SURF_H + 2.2;
const POLE = [-24, -4.5];
const POLE_TOP = SURF_H + 12; // 6 m at 1:50

const BOX = { x: -7, y: -23, w: 12.6, d: 12.6, r: 0.45 }; // an N150 mini PC, 12.6 × 12.6 × 4.4 cm
const FEET_Z = 0.3;
const TRAY_TOP = 2.3;
const SHELL_TOP = 4.7;
const WALL = 0.3;

const PHONE = { x: -8.5, y: 5, w: 7.2, d: 15, r: 1 };
const PHONE_H = 0.8;
const BUTTON = { x: PHONE.x + 0.7, y: PHONE.y + 2.4, w: 5.8, d: 3.2, r: 0.35 };
const RING = [PHONE.x + 1.55, PHONE.y + 7.45, 0.75];

const MOVES = { shell: [0, 0, 8], camera: [0, 0, 4.5], sign: [3.2, 0, 0] };
const SPRING = { order: ["shell", "camera", "sign"], stagger: 0.035, stiffness: 64, damping: 14.2, substeps: 4 };

let P = k.fitProjection(
  [
    ...k.boxCorners(PLATE, -PLATE_H - FOOT_H, 0),
    [POLE[0], POLE[1], POLE_TOP + 1.6 + MOVES.camera[2]],
    ...k.boxCorners(BOX, 0, SHELL_TOP + MOVES.shell[2]),
  ],
  WIDTH,
  HEIGHT,
  { pad: 26, azimuth: 45 },
);

const at = (point) => k.iso(point, P);
const S = (paths, style) => k.solidSvg(paths, style);
const L = (d, style) => k.lineSvg(d, style);
const D = (points, style) => k.dotsSvg(points, style);
const byDepth = (points) => [...points].sort((a, b) => k.depthOf([a[0], a[1], 0], P) - k.depthOf([b[0], b[1], 0], P));
const slab = (plan, z, h, steps = 3, bevel = 0) => k.slabOf(plan, z, h, P, steps, bevel);
const rect = (x0, y0, x1, y1, r = 0) => {
  const w = x1 - x0;
  const h = y1 - y0;
  return r ? `M${x0 + r} ${y0}H${x1 - r}A${r} ${r} 0 0 1 ${x1} ${y0 + r}V${y1 - r}A${r} ${r} 0 0 1 ${x1 - r} ${y1}H${x0 + r}A${r} ${r} 0 0 1 ${x0} ${y1 - r}V${y0 + r}A${r} ${r} 0 0 1 ${x0 + r} ${y0}Z` : `M${x0} ${y0}h${w}v${h}h${-w}Z`;
};
const circle = (cx, cy, r) => `M${cx - r} ${cy}a${r} ${r} 0 1 0 ${r * 2} 0a${r} ${r} 0 1 0 ${-r * 2} 0Z`;
// Lines drawn in a face's own plane keep the figure's stroke weight under the face transform.
const onFace = (matrix, paths) => `<g transform="${matrix}">${paths.map(([d, tone, free]) => `<path class="iso-line" data-tone="${tone}"${free ? ' data-free=""' : ""} vector-effect="non-scaling-stroke" d="${d}"/>`).join("")}</g>`;

function plate() {
  const out = [`<path class="iso-halo" d="${k.haloOf(PLATE, -PLATE_H - FOOT_H, PLATE_H, P)}"/>`];
  for (const [x, y] of byDepth(k.corners(PLATE, 6))) {
    out.push(S(k.cylinder(x, y, 2.6, -PLATE_H - FOOT_H, 0.5, P, 28), { tone: "lo" }));
    out.push(S(k.cylinder(x, y, 2, -PLATE_H - FOOT_H + 0.5, FOOT_H - 0.5, P, 28), { tone: "mid" }));
  }
  out.push(S(slab(PLATE, -PLATE_H, PLATE_H, 8, 0.8), { tone: "mid" }));
  out.push(L(k.planOutline(k.insetPlan(PLATE, 1.8), 0, P), { tone: "faint" }));
  const screws = k.corners(PLATE, 3.6);
  out.push(L(screws.map(([x, y]) => k.ring(x, y, 0.8, 0, P, 16)).join(""), { tone: "lo" }));
  out.push(D(screws.map(([x, y]) => at([x, y, 0])), { size: 0.5 }));

  const strip = { x: -11, y: PLATE.y + PLATE.d - 4.6, w: 18, d: 2.2, r: 0.3 };
  out.push(S(slab(strip, 0, 0.2), { tone: "lo", crease: "none" }));
  const ticks = k.topTicks(strip.x + 1, strip.x + strip.w - 1, 1, 5, strip.y + 0.25, 0.2, [0.6, 1.1], P);
  out.push(L(ticks.minor, { tone: "lo" }), L(ticks.major, { tone: "mid" }));
  out.push(D([at([strip.x + 0.5, strip.y + 1.6, 0.2]), at([strip.x + strip.w - 0.5, strip.y + 1.6, 0.2])], { size: 0.4 }));

  const label = { x: -8.2, y: 22.4, w: 7.6, d: 5.4, r: 0.4 };
  out.push(S(slab(label, 0, 0.25, 4), { tone: "lo", crease: "none" }));
  out.push(L(k.planOutline(k.insetPlan(label, 0.5), 0.25, P, 4), { tone: "faint" }));
  out.push(k.faceTextSvg(k.topMatrix([label.x + 1.1, label.y + 2.5], 0.25, P, "x"), "1:50", { size: 1.5, tone: "mid" }));
  for (const [y, w] of [[3.3, 5.4], [3.9, 4.2], [4.5, 3]]) out.push(L(k.lineOnTop([label.x + 1.1, label.y + y], [label.x + 1.1 + w, label.y + y], 0.25, P), { tone: "lo", free: true }));
  out.push(D([at([label.x + label.w - 1.1, label.y + 1.1, 0.25]), at([label.x + label.w - 1.1, label.y + label.d - 1.1, 0.25])], { size: 0.4 }));
  return out.join("");
}

function court() {
  const out = [S(slab(SURF, 0, SURF_H, 4, 0.4), { tone: "mid" })];
  const z = SURF_H;
  const { x0, x1, y0, y1 } = COURT;
  const mid = (x0 + x1) / 2;
  out.push(L(k.onTop([[x0, y0], [x1, y0], [x1, y1], [x0, y1]], z, P, true), { tone: "hi" }));
  out.push(L([NET_Y - KITCHEN, NET_Y + KITCHEN].map((y) => k.lineOnTop([x0, y], [x1, y], z, P)).join(""), { tone: "mid" }));
  out.push(L([k.lineOnTop([mid, y0], [mid, NET_Y - KITCHEN], z, P), k.lineOnTop([mid, NET_Y + KITCHEN], [mid, y1], z, P)].join(""), { tone: "mid" }));
  out.push(L(k.lineOnTop([POSTS[0], NET_Y], [POSTS[1], NET_Y], z, P), { tone: "faint" }));
  return out.join("");
}

function pole() {
  const [x, y] = POLE;
  const out = [];
  out.push(S(k.cylinder(x, y, 1.1, SURF_H, 0.3, P, 28), { tone: "lo" }));
  out.push(D([0, 1, 2, 3].map((i) => at([x + 0.75 * Math.cos(i * Math.PI / 2 + Math.PI / 4), y + 0.75 * Math.sin(i * Math.PI / 2 + Math.PI / 4), SURF_H + 0.3])), { size: 0.4 }));
  out.push(S(k.cylinder(x, y, 0.32, SURF_H + 0.3, POLE_TOP - SURF_H - 0.3, P, 20), { tone: "mid", crease: "none" }));
  out.push(L([4, 7, 10].map((h) => k.sideArc(x, y, 0.32, SURF_H + h, P, 12)).join(""), { tone: "faint" }));
  // junction box where the camera's network cable leaves the pole
  out.push(S(slab({ x: x + 0.3, y: y - 0.5, w: 0.7, d: 1, r: 0.1 }, SURF_H + 1, 1.2), { tone: "mid" }));
  out.push(S(k.cylinder(x, y, 0.45, POLE_TOP, 0.25, P, 20), { tone: "mid" }));
  return out.join("");
}

function camera() {
  const [x, y] = POLE;
  const z = POLE_TOP + 0.25;
  const out = [];
  out.push(S(slab({ x: x - 0.3, y: y - 0.2, w: 0.6, d: 1.4, r: 0.15 }, z, 0.4), { tone: "mid" }));
  out.push(S(slab({ x: x - 0.25, y: y + 0.9, w: 0.5, d: 0.5, r: 0.1 }, z + 0.4, 0.4), { tone: "mid" }));
  const body = { x: x - 0.9, y: y + 1.1, w: 1.8, d: 3.4, r: 0.6 };
  out.push(S(slab(body, z + 0.8, 1.3, 6, 0.3), { tone: "hi", crease: "lo", inner: L(k.sideSeam(body, z + 1.45, P, 6), { tone: "faint" }) }));
  out.push(S(slab({ x: x - 1.1, y: y + 0.9, w: 2.2, d: 4.2, r: 0.4 }, z + 2.1, 0.18, 4), { tone: "mid" }));
  const face = y + 4.5;
  out.push(L(k.sideRing([x, face, z + 1.45], 0.5, "y", P, 24), { tone: "mid" }));
  out.push(L(k.sideRing([x, face, z + 1.45], 0.26, "y", P, 20), { tone: "hi" }));
  out.push(D([at([x + 0.65, face, z + 1.85])], { size: 0.55, tone: "hi", pulse: true }));
  return out.join("");
}

function post(x) {
  const out = [];
  out.push(S(k.cylinder(x, NET_Y, 0.7, SURF_H, 0.2, P, 20), { tone: "lo" }));
  out.push(S(k.cylinder(x, NET_Y, 0.35, SURF_H + 0.2, POST_TOP - SURF_H - 0.2, P, 20), { tone: "mid", crease: "none" }));
  out.push(S(k.cylinder(x, NET_Y, 0.45, POST_TOP, 0.18, P, 20), { tone: "mid" }));
  return out.join("");
}

function net() {
  const [from, to] = [POSTS[0] + 0.35, POSTS[1] - 0.35];
  const bottom = SURF_H + 0.35;
  const top = SURF_H + 1.75; // 0.91 m at the posts
  const out = [S(slab({ x: from, y: NET_Y - 0.08, w: to - from, d: 0.16, r: 0 }, bottom, top - bottom, 2), { tone: "lo", crease: "none" })];
  const face = NET_Y + 0.08;
  const mesh = [];
  for (let x = from + 0.6; x < to - 0.3; x += 0.6) mesh.push(k.segment([x, face, bottom], [x, face, top], P));
  for (let z = bottom + 0.35; z < top - 0.1; z += 0.35) mesh.push(k.segment([from, face, z], [to, face, z], P));
  out.push(L(mesh.join(""), { tone: "faint" }));
  out.push(S(slab({ x: from, y: NET_Y - 0.14, w: to - from, d: 0.28, r: 0.05 }, top, 0.22, 2), { tone: "hi", crease: "none" }));
  return out.join("");
}

function sign() {
  const x = POSTS[1];
  const face = x + 0.75;
  const plan = { x: x + 0.45, y: NET_Y - 0.9, w: 0.3, d: 1.8, r: 0.08 };
  const z0 = SURF_H + 0.5;
  const z1 = POST_TOP + 0.3;
  const out = [];
  out.push(S(slab({ x: x + 0.3, y: NET_Y - 0.25, w: 0.2, d: 0.5, r: 0 }, z0 + 0.4, z1 - z0 - 0.8, 2), { tone: "mid", crease: "none" }));
  out.push(S(slab(plan, z0, z1 - z0, 2, 0.12), { tone: "hi", crease: "lo" }));
  // the court code, drawn in the sign's own plane (local x runs along −y, local y down from the top edge)
  const q = 0.55;
  const top = 0.28;
  const finder = (cx, cy) => [rect(cx, cy, cx + 0.32, cy + 0.32), "hi"];
  out.push(onFace(k.sideMatrix([face, NET_Y], z1, P, "right"), [
    [rect(-q, top, q, top + q * 2), "mid"],
    finder(-q + 0.08, top + 0.08),
    finder(q - 0.4, top + 0.08),
    finder(-q + 0.08, top + q * 2 - 0.4),
    [rect(0.05, top + 0.62, 0.2, top + 0.77), "hi"],
    [rect(0.25, top + 0.85, 0.4, top + 1.0), "hi"],
    [rect(-0.1, top + 0.8, 0.05, top + 0.95), "hi"],
    ["M-0.45 1.62H0.45", "lo", true],
  ]));
  return out.join("");
}

function cable() {
  const [x, y] = POLE;
  const route = [
    [x + 1, y, SURF_H + 1.3],
    [x + 1.6, y, SURF_H],
    [SURF.x + SURF.w, y, SURF_H],
    [SURF.x + SURF.w + 0.4, y, 0],
    [BOX.x + 11, y, 0],
    [BOX.x + 11, BOX.y + BOX.d + 1.8, 0],
    [BOX.x + 11, BOX.y + BOX.d + 0.6, 1.05],
    [BOX.x + 11, BOX.y + BOX.d, 1.05],
  ].map(at);
  return `<g style="--anatomy-weight:1.1px">${k.wireSvg(route[0], route[route.length - 1], { via: route.slice(1, -1), tone: "mid", size: 0.9 })}</g>`;
}

function box() {
  const out = [`<path class="iso-halo" d="${k.haloOf(k.insetPlan(BOX, 1), 0, 0.1, P)}" style="filter:blur(5px)"/>`];
  for (const [x, y] of byDepth(k.corners(BOX, 1.6))) out.push(S(k.cylinder(x, y, 0.6, 0, FEET_Z, P, 16), { tone: "lo" }));
  const { x, y, w, d } = BOX;
  out.push(S(slab(BOX, FEET_Z, 0.5, 6), { tone: "mid" }));
  out.push(S(slab({ x, y, w: WALL, d, r: 0 }, FEET_Z, TRAY_TOP - FEET_Z, 2), { tone: "mid" }));
  out.push(S(slab({ x, y, w, d: WALL, r: 0 }, FEET_Z, TRAY_TOP - FEET_Z, 2), { tone: "mid" }));
  out.push(inside());
  out.push(S(slab({ x: x + w - WALL, y, w: WALL, d, r: 0 }, FEET_Z, TRAY_TOP - FEET_Z, 2), { tone: "mid" }));
  out.push(S(slab({ x, y: y + d - WALL, w, d: WALL, r: 0 }, FEET_Z, TRAY_TOP - FEET_Z, 2), { tone: "mid" }));
  // ports on the front wall, in its own plane (local x along +x from the left edge, local y down from the tray top)
  out.push(onFace(k.sideMatrix([x, y + d], TRAY_TOP, P, "left"), [
    [circle(1.2, 0.95, 0.38), "mid"],
    [circle(1.2, 0.95, 0.16), "lo"],
    [rect(2.2, 0.8, 3.1, 1.1, 0.14), "mid"],
    [rect(3.8, 0.7, 5.0, 1.15), "mid"],
    [rect(5.6, 0.7, 6.8, 1.15), "mid"],
    [circle(7.7, 0.95, 0.22), "mid"],
    [rect(10.2, 0.45, 11.8, 1.75, 0.08), "hi"],
    [rect(10.65, 1.75, 11.35, 1.95), "hi"],
  ]));
  out.push(D([at([x + 1.9, y + d, TRAY_TOP - 0.95])], { size: 0.5, tone: "hi", pulse: true }));
  return out.join("");
}

function inside() {
  const z = FEET_Z + 0.5;
  const { x, y } = BOX;
  const out = [];
  const board = { x: x + 0.6, y: y + 0.6, w: 11.4, d: 11.4, r: 0.3 };
  out.push(S(slab(board, z, 0.15, 3), { tone: "lo", crease: "none" }));
  const top = z + 0.15;
  out.push(L(k.corners(board, 0.7).map(([px, py]) => k.ring(px, py, 0.32, top, P, 12)).join(""), { tone: "lo" }));
  out.push(D(k.corners(board, 0.7).map(([px, py]) => at([px, py, top])), { size: 0.4 }));
  // the N150's heatsink: a base and fins
  const sink = { x: x + 1.6, y: y + 1.4, w: 5.2, d: 5.2, r: 0.2 };
  out.push(S(slab(sink, top, 0.45, 2), { tone: "mid" }));
  for (let i = 0; i < 13; i++) out.push(S(slab({ x: sink.x + 0.3, y: sink.y + 0.3 + i * 0.37, w: sink.w - 0.6, d: 0.1, r: 0 }, top + 0.45, 0.95, 2), { tone: "mid", crease: "none" }));
  // blower fan
  const [fx, fy, fr] = [x + 9.1, y + 4.1, 2.2];
  out.push(S(k.cylinder(fx, fy, fr, top, 1.5, P, 40, 0.2), { tone: "mid" }));
  const ft = top + 1.5;
  out.push(L(k.ring(fx, fy, fr - 0.35, ft, P, 40) + k.ring(fx, fy, 0.75, ft, P, 20), { tone: "lo" }));
  out.push(L(Array.from({ length: 9 }, (_, i) => {
    const a = (i / 9) * Math.PI * 2;
    const p = (r, t) => [fx + r * Math.cos(a + t), fy + r * Math.sin(a + t)];
    return k.onTop([p(0.75, 0), p(1.3, 0.25), p(fr - 0.35, 0.55)], ft, P);
  }).join(""), { tone: "lo" }));
  out.push(D([at([fx, fy, ft])], { size: 0.5 }));
  // memory and the SSD
  const ram = { x: x + 1.4, y: y + 7.4, w: 6.6, d: 2.8, r: 0.15 };
  out.push(S(slab(ram, top, 0.35, 2), { tone: "mid" }));
  for (let i = 0; i < 4; i++) out.push(S(slab({ x: ram.x + 0.4 + i * 1.6, y: ram.y + 0.6, w: 1.2, d: 1.5, r: 0.05 }, top + 0.35, 0.18, 2), { tone: "lo", crease: "none" }));
  const ssd = { x: x + 9.2, y: y + 7.6, w: 2.2, d: 3.6, r: 0.15 };
  out.push(S(slab(ssd, top, 0.2, 2), { tone: "mid" }));
  out.push(S(slab({ x: ssd.x + 0.4, y: ssd.y + 0.5, w: 1.4, d: 1.4, r: 0.05 }, top + 0.2, 0.15, 2), { tone: "lo", crease: "none" }));
  out.push(D([at([ssd.x + 1.1, ssd.y + 3.2, top + 0.2])], { size: 0.45 }));
  return out.join("");
}

function shell() {
  const { x, y, w, d } = BOX;
  const out = [];
  out.push(S(slab(BOX, TRAY_TOP, SHELL_TOP - TRAY_TOP, 6, 0.5), { tone: "mid", inner: L(k.sideSeam(BOX, SHELL_TOP - 0.45, P, 6), { tone: "faint" }) }));
  out.push(L(k.planOutline(k.insetPlan(BOX, 2.2), SHELL_TOP, P, 4), { tone: "faint" }));
  out.push(D(k.dotGrid(x + 3.3, y + 3.3, 9, 9, 0.75, SHELL_TOP, P), { size: 0.38, tone: "lo" }));
  // side vents on the right face (local x along −y from the front edge)
  out.push(onFace(k.sideMatrix([x + w, y + d], SHELL_TOP, P, "right"), Array.from({ length: 8 }, (_, i) => [rect(2.4 + i * 1.1, 0.75, 2.9 + i * 1.1, 1.8, 0.2), "lo"])));
  return out.join("");
}

function phone() {
  const z = PHONE_H;
  const out = [`<path class="iso-halo" d="${k.haloOf(k.insetPlan(PHONE, 0.6), 0, 0.1, P)}" style="filter:blur(4px)"/>`];
  out.push(S(slab(PHONE, 0, z, 8, 0.25), { tone: "hi", crease: "lo" }));
  out.push(S(slab({ x: PHONE.x + PHONE.w - 0.05, y: PHONE.y + 3.2, w: 0.12, d: 1.9, r: 0.05 }, 0.25, 0.3, 2), { tone: "mid", crease: "none" }));
  const glass = k.insetPlan(PHONE, 0.35);
  out.push(L(k.planOutline(glass, z, P, 6), { tone: "lo" }));
  out.push(L(k.planOutline({ x: PHONE.x + 2.9, y: PHONE.y + 0.75, w: 1.4, d: 0.22, r: 0.11 }, z, P, 3), { tone: "lo" }));
  out.push(D([at([PHONE.x + 4.8, PHONE.y + 0.86, z])], { size: 0.45 }));
  // the court page
  const left = BUTTON.x;
  out.push(L(k.lineOnTop([left, PHONE.y + 1.75], [left + 2.6, PHONE.y + 1.75], z, P), { tone: "mid", free: true }));
  out.push(S(slab(BUTTON, z, 0.12, 3), { tone: "hi", lit: true, crease: "none" }));
  out.push(`<g class="ch-on-lit">${k.faceTextSvg(k.topMatrix([BUTTON.x + BUTTON.w / 2, BUTTON.y + BUTTON.d / 2 + 0.45], z + 0.12, P, "x"), "Clip that", { size: 1.25, tone: "hi", anchor: "middle" })}</g>`);
  const [rx, ry, rr] = RING;
  out.push(L(k.ring(rx, ry, rr, z, P, 32), { tone: "faint" }));
  out.push(`<path data-part="ring" class="iso-line" data-tone="lit" d="" style="stroke-width:1.3"/>`);
  out.push(L([[1.0, 3.2], [1.6, 2.1]].map(([dy, w]) => k.lineOnTop([rx + 1.3, ry - 0.35 + dy - 0.65], [rx + 1.3 + w, ry - 0.35 + dy - 0.65], z, P)).join(""), { tone: "lo", free: true }));
  out.push(L(k.lineOnTop([left, PHONE.y + 9.6], [left + 2.2, PHONE.y + 9.6], z, P), { tone: "mid", free: true }));
  for (const row of [PHONE.y + 10.4, PHONE.y + 12.3]) {
    const thumb = { x: left, y: row, w: 2.4, d: 1.5, r: 0.12 };
    out.push(L(k.planOutline(thumb, z, P, 3), { tone: "lo" }));
    out.push(L(k.onTop([[left + 0.95, row + 0.4], [left + 1.55, row + 0.75], [left + 0.95, row + 1.1]], z, P, true), { tone: "mid" }));
    out.push(L([0.45, 1.05].map((dy, i) => k.lineOnTop([left + 3, row + dy], [left + 3 + (i ? 1.6 : 2.6), row + dy], z, P)).join(""), { tone: "lo", free: true }));
  }
  out.push(L(k.lineOnTop([PHONE.x + 2.6, PHONE.y + 14.35], [PHONE.x + 4.6, PHONE.y + 14.35], z, P), { tone: "lo", free: true }));
  return out.join("");
}

const part = (name, svg) => `<g data-part="${name}">${svg}</g>`;
const label =
  "A 1:50 model of a pickleball court on a bench, with its camera high on a pole behind one baseline and the court sign on the net post. A network cable runs from the pole to the real court box beside it, and a phone lies in front showing the court page with the yellow Clip that button.";
const scene = () => [plate(), court(), pole(), part("camera", camera()), cable(), post(POSTS[0]), net(), post(POSTS[1]), part("sign", sign()), box(), part("shell", shell()), phone()];
const body = scene();

// Laminated-sign palette (DESIGN.md): cream paper, ink lines, one highlighter yellow for the action.
const PALETTE = `
.iso{--anatomy-lit-top:#ffe14d;--anatomy-lit-shade:#d9bb2c;--anatomy-lit:#fff3a8}
.iso[data-theme="light"]{--anatomy-page:#f6f4ee;--anatomy-card:#fffdf7;--anatomy-text:#4a4a46;--anatomy-strong:#1b1b1b;--anatomy-muted:#8b877c;
--anatomy-paper:#fffdf7;--anatomy-face:#fffdf7;--anatomy-top:#fffefa;--anatomy-shade-0:#e2ddd0;--anatomy-shade-1:#ebe7dc;--anatomy-shade-2:#f2efe6;--anatomy-shade-3:#f8f6ef;
--anatomy-hi:#1b1b1b;--anatomy-mid:#77736a;--anatomy-lo:#aaa596;--anatomy-faint:#d8d3c6;--anatomy-lit:#1b1b1b;--anatomy-lit-top:#ffe14d;--anatomy-lit-shade:#f0cd2c;
--anatomy-halo:#8b877c;--anatomy-dot:#2e2d2a}
.iso-face-text{font-family:Arial,Helvetica,sans-serif;font-weight:700;letter-spacing:0}
.ch-on-lit .iso-face-text{fill:#1b1b1b}
`;
const withPalette = (html) => html.replace("</style>", `${PALETTE}</style>`);

const ringPath = (share) => {
  const [rx, ry, rr] = RING;
  const steps = Math.max(2, Math.ceil(share * 48));
  const points = Array.from({ length: steps + 1 }, (_, i) => {
    const a = (i / steps) * share * Math.PI * 2;
    return [rx + rr * Math.sin(a), ry - rr * Math.cos(a)];
  });
  return share > 0 ? k.onTop(points, PHONE_H, P) : "";
};

const live = `
const P = ${JSON.stringify(P)};
const RING = ${JSON.stringify(RING)}, Z = ${PHONE_H};
const MOVES = ${JSON.stringify(MOVES)};
const SPRING = ${JSON.stringify(SPRING)};
const BEFORE = ${BEFORE}, AFTER = ${AFTER}, SAVE_SECS = ${SAVE_SECS};
const stage = document.querySelector(".iso-stage");
const svg = stage.querySelector("svg");
const ringLine = svg.querySelector('[data-part="ring"]');
const parts = Object.fromEntries(SPRING.order.map((name) => [name, svg.querySelector('[data-part="' + name + '"]')]));
const readout = document.querySelector("[data-readout]");
const still = matchMedia("(prefers-reduced-motion: reduce)");
const springs = Object.fromEntries(SPRING.order.map((name) => [name, { at: 0, rate: 0, target: 0, pending: null }]));
const ringPath = ${ringPath.toString().replace(/k\.onTop/g, "onTop").replace("PHONE_H", "Z")};
let clip = null, lastLength = 0, readyAt = -1, apart = false;
let frame = 0, last = 0, clock = 0, now = 0, touring = !still.matches, tourClock = 0, visible = false, idleTimer = 0;

function setApart(next) {
  if (next === apart) return;
  apart = next;
  const moving = (apart ? SPRING.order : [...SPRING.order].reverse()).filter((name) => springs[name].target !== (apart ? 1 : 0) || springs[name].pending);
  moving.forEach((name, index) => { springs[name].pending = { value: apart ? 1 : 0, at: clock + index * SPRING.stagger }; });
  stage.setAttribute("aria-valuetext", apart ? "Apart: the camera, the court sign and the box's lid are lifted off" : "Assembled");
  run();
}
function tap() {
  if (clip && now - clip.last <= AFTER) clip.last = now;
  else if (!clip) clip = { first: now, last: now };
  else return;
  run();
}
function stepSprings(dt, calm) {
  let moving = false;
  for (const name of SPRING.order) {
    const s = springs[name];
    if (s.pending && clock >= s.pending.at) { s.target = s.pending.value; s.pending = null; }
    if (calm) { s.at = s.target; s.rate = 0; }
    else for (let i = 0; i < SPRING.substeps; i++) {
      const h = dt / SPRING.substeps;
      s.rate += ((s.target - s.at) * SPRING.stiffness - s.rate * SPRING.damping) * h;
      s.at += s.rate * h;
    }
    if (Math.abs(s.target - s.at) < 5e-4 && Math.abs(s.rate) < 5e-4) { s.at = s.target; s.rate = 0; }
    if (s.at !== s.target || s.pending) moving = true;
  }
  return moving;
}
function draw() {
  for (const name of SPRING.order) {
    const share = springs[name].at;
    if (share) parts[name].setAttribute("transform", translateAlong(P, MOVES[name].map((v) => v * share)));
    else parts[name].removeAttribute("transform");
  }
  let text;
  if (clip) {
    const done = clip.last + SAVE_SECS;
    const share = Math.min(1, (now - clip.first) / (done - clip.first));
    ringLine.setAttribute("d", ringPath(share));
    const length = BEFORE + (clip.last - clip.first) + AFTER;
    text = "saving clip · " + length.toFixed(0) + " s · " + Math.max(0, done - now).toFixed(0) + " s left";
    if (share >= 1) { lastLength = length; clip = null; readyAt = now; }
  }
  if (!clip) {
    if (readyAt >= 0 && now - readyAt < 4) text = "clip ready · " + lastLength.toFixed(0) + " s · wide + vertical";
    else { ringLine.setAttribute("d", ""); text = apart ? "apart · camera, sign, box lid" : "recording · court 1"; }
  }
  if (readout.textContent !== text) readout.textContent = text;
}
const TOUR = [[1.2, () => tap()], [14.5, () => setApart(true)], [18, () => setApart(false)]];
const TOUR_LENGTH = 21;
function tick(stamp) {
  const dt = last ? Math.min((stamp - last) / 1000, 1 / 30) : 1 / 60;
  last = stamp;
  clock += dt;
  now += dt;
  if (touring) {
    const before = tourClock;
    tourClock += dt;
    for (const [t, act] of TOUR) if (before < t && tourClock >= t) act();
    if (tourClock >= TOUR_LENGTH) tourClock = 0;
  }
  const moving = stepSprings(dt, still.matches);
  draw();
  const busy = touring || moving || clip || (readyAt >= 0 && now - readyAt < 4.1);
  frame = visible && busy ? requestAnimationFrame(tick) : 0;
}
const run = () => { if (!frame && visible) { last = 0; frame = requestAnimationFrame(tick); } };
const hold = () => { touring = false; clearTimeout(idleTimer); idleTimer = setTimeout(() => { if (!still.matches) { touring = true; tourClock = 0; setApart(false); run(); } }, 3600); };
new IntersectionObserver((entries) => { visible = entries[entries.length - 1].isIntersecting; if (visible) run(); }, { rootMargin: "120px 0px" }).observe(stage);
stage.addEventListener("pointerenter", (event) => { if (event.pointerType === "mouse") { hold(); setApart(true); } });
stage.addEventListener("pointerleave", (event) => { if (event.pointerType === "mouse") { hold(); setApart(false); } });
stage.addEventListener("click", () => { hold(); tap(); });
stage.addEventListener("keydown", (event) => {
  if (event.key === " " || event.key === "Enter") { event.preventDefault(); hold(); tap(); }
  else if (event.key === "ArrowUp") { event.preventDefault(); hold(); setApart(true); }
  else if (event.key === "ArrowDown" || event.key === "Escape") { event.preventDefault(); hold(); setApart(false); }
});
draw();
`;

const plateBody = k.plateHtml({
  fig: "Fig 1",
  title: "Court Highlights",
  hint: "Hover to take apart · tap to clip",
  readout: "recording · court 1",
  keys: [
    { mark: "lit", label: "Clip that: one tap on the court page" },
    { mark: "raised", label: "Court box: records every court it serves" },
    { mark: "edge", label: "One network cable per camera" },
  ],
  caption: `A 1:50 model court beside the real court box and a phone. The camera on the pole records the court through opening hours; a tap on Clip that saves the ${BEFORE} s before and ${AFTER} s after, ready in about ${SAVE_SECS} s, and a second tap within ${AFTER} s makes it longer.`,
  body: `<div class="iso-stage" tabindex="0" role="group" aria-label="Court Highlights: Space or Enter taps Clip that; Up arrow takes the camera, the sign and the box lid off, Down arrow puts them back" aria-valuetext="Assembled">${k.figureSvg({ width: WIDTH, height: HEIGHT, label, body })}</div>`,
});
// The page inlines the kit's source, so its MIT notice travels with it.
const notice = `/* The Anatomy kit inlined below: ${readFileSync(join(SKILL, "LICENSE"), "utf8").replaceAll("*/", "")} */\n`;
const page = k.pageHtml({ title: "Court Highlights", theme, body: plateBody, script: notice + kitScript() + live, width: 720 });
const name = theme === "light" ? "court-figure-light.html" : "court-figure.html";
writeFileSync(join(HERE, name), withPalette(page));

// The README's stills: assembled and framed tight (no room kept for the explode), with a clip two thirds saved.
const STILL_HEIGHT = 380;
P = k.fitProjection([...k.boxCorners(PLATE, -PLATE_H - FOOT_H, 0), [POLE[0], POLE[1], POLE_TOP + 1.6], ...k.boxCorners(BOX, 0, SHELL_TOP)], WIDTH, STILL_HEIGHT, { pad: 10, azimuth: 45 });
const still = scene();
for (const scheme of ["light", "dark"]) {
  const svg = k.figureSvg({ width: WIDTH, height: STILL_HEIGHT, label, body: still, standalone: scheme }).replace('d="" style="stroke-width:1.3"', `d="${ringPath(0.66)}" style="stroke-width:1.3"`);
  writeFileSync(join(HERE, "..", `court-figure-${scheme}.svg`), withPalette(svg));
}
console.log("wrote", name, "and the README stills");

# Design direction source — Court Highlights (for `/forge:design system`)

Approved in client-review revisions 13–14 and 18 (A-28, A-107, A-108, A-110, A-112). This is the **source** the build
turns into DESIGN.md; it is not itself DESIGN.md. Throwaway studies: `wireframes/d3-laminated-sign-*.png`.

**Design read:** court page as an Operate surface for players on their own phones, outdoors or under hall lights,
sweaty and mid-game, one hand, seconds of attention; staff and fleet consoles as Operate surfaces used occasionally
(open/close checks, content picking, requests), leaning toward the venue's own printed court sign.

## Direction — D3 · Laminated sign (chosen by the owner over D1 court-blue and D2 scoreboard)

- **Thesis:** the screen looks like the laminated sign hanging on the net post — the same object the player just scanned.
  Refuses the category default of a dark sports "tech" dashboard.
- **Colour strategy:** Restrained. Off-white ground (~#f6f4ee), near-black ink (~#1b1b1b) for text and 2–3 px rules,
  **one** highlighter-yellow action colour (~#ffe14d) used only for the primary action (Clip that, Start game) and the
  current selection; error red (~#a4161a) and OK green (~#1f6b3a) for status only. Never pure #000/#fff.
  Body text and the Clip that label ≥ 7:1 contrast (NFR-6, confirmed at sign-off).
- **Type:** one plain workhorse sans with heavy weights (sign-like): court name uppercase, very bold; tabular numerals
  for timers and times.
- **Shape:** one small radius (≈ 4 px); heavy ink borders instead of shadows; flat, no gradients, no glow.
- **Density:** court page airy, actions ≥ 48 px, one primary action per state; consoles medium density (tables of clips,
  50 per page), standard side navigation (Courts, Clips, Footage, Staff, Settings, Help).
- **One fixed look for every venue** (A-110): venues do not change colours; the venue logo sits at the top of the court
  page and appears in emails, signs and burned into clips (A-107).
- **Court page order** (A-112): venue logo + court name + recording status → Clip that → game panel (Start/Stop, timer,
  "venue may post") → "Send our videos to" emails → clips on this court.

## Imagery (A-107)

Real camera stills (clip thumbnails, setup preview) and the venue logo. Two authored black-line drawings in the sign
style: (1) setup wizard — a pickleball court with the camera mounted high at the centre (5–7 m) and the court sign on the
net post; (2) empty clip library — a phone scanning a court code. No other decorative art on task screens.

## Motion (A-108)

Only the saving ring: after a tap it fills over the clip's remaining window with "Saving… ready in N s", plus one
vibration where the phone allows. With prefers-reduced-motion: static ring, text countdown only. Nothing else animates;
state changes 150–250 ms at most.

## Dislikes (from the artifact-reaction loop)

- Dark scoreboard panels and neon/amber-on-black (D2 — not chosen; also weak in sun).
- Court-blue filled navigation bars and ball-yellow-green buttons (D1 — not chosen).
- The plain grey admin-tool look (offered, not chosen).

## Voice

Plain, short, sentence case, no exclamation marks; say what happened and what to do ("Court 2 camera is offline —
clips aren't available"; "These videos have expired"). Terms from SRS §2 (court code, court page, Clip that, keep, hold).

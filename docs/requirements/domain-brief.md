# Domain brief — court replay / highlight cameras (Philippines)

Run: `forge/requirements-261020-1145` · checked **2026-10-20** · jurisdiction assumed **Philippines** (brief cites
rallycam.ph; not yet confirmed — A-2/A-54). Research informs proposals; nothing here is a client requirement until
reviewed in `client-review.md`. Legal applicability is **not** asserted — it stays open pending a responsible reviewer.

## Sources (accessed 2026-10-20)

| ID | Source | Tier / bias | What it gave |
|---|---|---|---|
| S-01 | rallycam.ph home page, rendered with Playwright → `evidence/S-01-rallycam-home.{txt,png}` | T4 vendor (the reference product) | Whole-product pattern quoted below |
| S-02 | pushitreplays.com/pickleball | T4 vendor | Button/app/watch triggers, 8–10 s before + 3–5 s after, 9:16 SmartCrop, per-court monthly SaaS |
| S-03 | visu.network/blog/pickleball-highlights + /products/visureplay | T4 vendor | "camera was already recording"; button saves window reaching back before the press |
| S-04 | broadcastnow.co.uk — PlaySight AI highlights for racquet sports | T3 trade press | AI rally/save/winner detection, clips bought in app (< €5) |
| S-05 | docs.rs/crate/retina, lib.rs/crates/retina-record | T3 project docs | Rust RTSP client used in production by Moonfire NVR; records camera H.264 to MP4 without re-encoding; API "not yet stable" |
| S-06 | Decorte et al., *Multi-Modal Hit Detection … in Padel*, CVPR Workshops 2024 | T1 peer-reviewed workshop | Audio-based hit detection, avg F1 0.92 on padel — rally detection is feasible from audio |
| S-07 | NPC Circular No. 2024-02, *CCTV Systems* (privacy.gov.ph PDF) | T1 regulator — **primary text blocked (HTTP 403 to scripted fetch)** | Next step: download by hand in a browser and confirm §3/§6 |
| S-08 | Conventus Law / DivinaLaw summary of NPC Circular 2024-02 (2025-10-03) | T2 law-firm secondary | §3: purpose defined and disclosed *before* cameras are used; "should not record audio"; retain only as long as necessary; signage; §6: access requests, 30-day abandonment, preserve footage while a request is pending |
| S-09 | RA 4200 Anti-Wiretapping Act (chanrobles full text; ndvlaw.com commentary) | T1 statute / T3 commentary | Recording private spoken words without authorization of all parties is unlawful; 6 months–6 years |
| S-10 | RA 10173 Data Privacy Act of 2012 + IRR | T1 statute | Video of identifiable people is personal information; transparency, legitimate purpose, proportionality; data-subject rights; security measures |

## (a) Table-stakes features for the category

Pattern that recurs across S-01..S-04:

1. **Always-rolling footage per court** — "The camera was already running" (S-01, S-03). Saving is retroactive.
2. **One-tap "save the last N seconds"** — RallyCam: last **30 s** (S-01); PUSHIT: **8–10 s before + 3–5 s after** (S-02).
3. **Trigger without an app** — RallyCam: QR "on the net post" opens a browser page, "No app, no account, no sign-up" (S-01).
   Others use a physical post button, app button or smartwatch (S-02).
4. **Whole-game recording** — tap record marks the start, stop cuts the game (S-01).
5. **Venue branding burned into every clip** — logo; optional sponsor / score (S-01, S-02).
6. **Vertical 9:16 social format** — PUSHIT crops and follows the ball (S-02). Ball-following needs computer vision.
7. **Automatic highlights** — the box finds rallies itself (S-01, S-04). Research shows audio hit detection works (S-06).
8. **Fast delivery** — "your best rally is on your phone before you have finished your water" (S-01).
9. **Auto-expiry** — "Videos delete themselves after three days unless a player saves them" (S-01).
10. **Venue dashboard** — cameras, courts, clips; clubhouse TV replay (S-02 "PUSHITV").
11. **Live streaming** (S-01, S-04) — needs internet; conflicts with an offline-only product.
12. **Court booking** (S-01) — a separate product line at RallyCam.

Hardware pattern: "One camera per court, and a small computer in a cupboard … an ordinary PoE security camera. The
computer holds the last few hours of footage on site, so only the finished clip ever leaves the building" (S-01).
PUSHIT mounts centrally at 5–7 m with a wide lens (S-02).

**Note on "offline":** RallyCam processes on site but still *delivers* over the internet (it sends the video to
each player's address). No source found a fully internet-free replay product — that is the gap this brief's
"offline" may target, and its delivery consequence (what a player does once they leave the venue) is open.

## (b) Glossary (proposed controlled vocabulary)

| Preferred term | Meaning | Avoid |
|---|---|---|
| Venue | The club / facility where courts are | tenant, site, org |
| Court | One playing area with its own camera | field, lane |
| Camera | One security-style network camera watching one court | cam, NVR channel |
| Court box (box) | The small computer at the venue that records and cuts video | server, NVR, edge node |
| Rolling footage | The last few hours a court box holds; older footage is overwritten | buffer, DVR, cache |
| Clip | A short saved piece, e.g. the last 30 s before a tap | highlight (when manual), replay |
| Game video | A recording from "start" to "stop" by players | session, match recording |
| Highlight reel | Clips the box finds by itself and joins together | auto-clips |
| Court code | The printed QR on the net post that opens that court's page | QR link |
| Clip that | The one-tap save button | record, capture |
| Branding | The venue logo burned into the video | watermark (ok in staff UI), overlay |
| Keep / expire | A kept clip survives auto-expiry; others are removed after the expiry period | archive, purge |
| Staff console | The venue staff's pages on the court box | admin panel, backend |

## (c) Regulatory checklist (applicability OPEN — needs the responsible reviewer)

| # | Obligation (as reported) | Source | Applies? | Product effect if it applies |
|---|---|---|---|---|
| R-1 | Video of identifiable players is personal information; venue is the personal information controller | S-10 | likely, open (A-54) | Purpose notice, proportionality, security, data-subject rights |
| R-2 | Purpose must be defined and disclosed **before** cameras are used; clear visible signage | S-08 (§3), S-07 to verify | open — is a highlights camera "CCTV" under the Circular? | Printed court sign with purpose, retention, contact |
| R-3 | CCTV "should not record audio" | S-08 (§3) | open | Default: no audio stored (A-37) |
| R-4 | Recording private spoken words without all parties' authorization is a crime | S-09 | open — on-court chatter is the risk | Default: no audio stored; any audio use needs legal review |
| R-5 | Retain only as long as necessary; storage capacity is not a justification | S-08 (§3) | open | Short rolling footage + clip expiry (A-17, A-18) |
| R-6 | Access requests: verified identity, date/time/location; preserve footage while pending; abandoned after 30 days | S-08 (§6) | open | Staff "hold" + find-by-court-and-time + export + log (SC-7) |
| R-7 | No cameras over restrooms / changing areas | S-08 (§3) | applies to camera placement | Install guide + sign-off checklist |
| R-8 | Children's footage (junior players) | — no source found | open, flagged | Ask the reviewer; may need parental notice |
| R-9 | Music added to clips is copyrighted (IP Code RA 8293) | general knowledge, no source pulled | open | Default: no music added (A-55) |
| R-10 | If players pay per clip: official receipts / tax | — not researched (no payments proposed) | N-A while clips are free (A-24) | — |

What still needs verification: the Circular's own text (S-07) — the primary PDF returned HTTP 403 to automated fetch
on 2026-10-20; download it in a browser and confirm §1 scope, §3 audio and retention, §6 access-request timing.

## (d) The 3–5 highest-risk questions a domain expert asks first

1. **Who is the customer?** Selling and installing boxes at many clubs (like RallyCam) or running one venue? This decides
   fleet installs and updates, who the data controller is, and whether a vendor console exists at all.
2. **What does "offline" mean for delivery?** With no internet, a phone gets a clip only while it is on the venue's Wi-Fi.
   Once a player leaves, the clip is out of reach unless they downloaded it. Is that acceptable?
3. **Audio — yes or no?** Audio makes highlights better and makes auto-detection easier (S-06), but carries RA 4200 and
   NPC risk (R-3, R-4).
4. **What box and how many courts per box?** Burning in a logo or making a vertical crop means re-encoding the video.
   That needs a hardware video encoder; cheap boards can fall behind with four cameras.
5. **Who may watch which clip?** Anyone on the venue Wi-Fi could reach the court pages. The court page must not become a
   way to watch strangers.

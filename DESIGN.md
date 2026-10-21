---
name: Court Highlights — Laminated sign
description: Every screen reads like the laminated sign on the net post; one highlighter action inside heavy ink lines.
mode: operate
status-cue: icon
colors:
  background: '#f6f4ee'
  surface: '#fffdf7'
  surface-container: '#ece8dd'
  on-surface: '#1b1b1b'
  on-surface-variant: '#4a4a46'
  primary: '#1b1b1b'
  on-primary: '#fffdf7'
  primary-container: '#ffe14d'
  on-primary-container: '#1b1b1b'
  outline: '#1b1b1b'
  outline-variant: '#8b877c'
  selection: '#ffe14d'
  error: '#a4161a'
  on-error: '#fffdf7'
  error-container: '#fbe2df'
  on-error-container: '#6d0e11'
  success: '#1f6b3a'
  success-container: '#dcefe1'
  on-success-container: '#123f22'
  warning: '#7a4f00'
  warning-container: '#fbefc9'
  on-warning-container: '#4d3200'
typography:
  display: { fontFamily: "Arial, Helvetica, sans-serif", fontSize: 30px, fontWeight: 800, lineHeight: 1.1, letterSpacing: 0.02em }
  headline: { fontFamily: "Arial, Helvetica, sans-serif", fontSize: 22px, fontWeight: 800, lineHeight: 1.2 }
  title: { fontFamily: "Arial, Helvetica, sans-serif", fontSize: 17px, fontWeight: 700, lineHeight: 1.3 }
  body: { fontFamily: "Arial, Helvetica, sans-serif", fontSize: 16px, fontWeight: 400, lineHeight: 1.5 }
  label: { fontFamily: "Arial, Helvetica, sans-serif", fontSize: 14px, fontWeight: 700, lineHeight: 1.25 }
  data: { fontFamily: "Arial, Helvetica, sans-serif", fontSize: 15px, fontWeight: 400, lineHeight: 1.4, fontVariation: tabular-nums }
  action: { fontFamily: "Arial, Helvetica, sans-serif", fontSize: 30px, fontWeight: 800, lineHeight: 1.1 }
spacing: { base: 4px, xs: 4px, sm: 8px, md: 16px, lg: 24px, xl: 40px, gutter: 16px }
rounded: { sm: 4px, pill: 9999px }
terms:
  Court box: [server, NVR, edge node]
  Rolling footage: [buffer, DVR]
  Clip: [replay]
  Clip that: [capture]
  Game: [session, match recording]
  Highlights: [auto-clips]
  Court code: [QR link]
  Court page: [portal]
  Keep: [archive, pin]
  Hold: [freeze]
  Staff console: [admin panel, backend]
  Venue: [tenant]
components:
  button-primary: { backgroundColor: "{colors.primary-container}", textColor: "{colors.on-primary-container}", rounded: "{rounded.sm}", padding: "12px 20px", height: 48px }
  button-clip-that: { backgroundColor: "{colors.primary-container}", textColor: "{colors.on-primary-container}", typography: "{typography.action}", rounded: "{rounded.sm}", padding: "24px 16px" }
  button-secondary: { backgroundColor: "{colors.surface}", textColor: "{colors.on-surface}", rounded: "{rounded.sm}", padding: "10px 16px", height: 44px }
  button-danger: { backgroundColor: "{colors.surface}", textColor: "{colors.error}", rounded: "{rounded.sm}", padding: "10px 16px", height: 44px }
  panel: { backgroundColor: "{colors.surface}", textColor: "{colors.on-surface}", rounded: "{rounded.sm}", padding: "16px" }
  chip: { backgroundColor: "{colors.surface}", textColor: "{colors.on-surface}", rounded: "{rounded.pill}", padding: "4px 10px" }
---
# Design System: Court Highlights — Laminated sign

## Overview

**Creative North Star: the laminated court sign.** The player has just scanned the sign hanging on the net post; the page
they land on is that same sign, continued: off-white card stock, heavy black rules, big plain type, and one stripe of
highlighter on the thing to press. Chosen by the owner over a court-blue scheme and a dark scoreboard (client review
A-28); one fixed look at every venue, with the venue's logo at the top (A-110).

Key characteristics: readable in outdoor sun (body text ≥ 7:1, NFR-6); one primary action per state, 48 px or larger on
the court page; ink rules instead of shadows; nothing animates except the saving ring. Mode: **Operate** on every
surface (court page, staff console, fleet console, the take-home page). Dials: variance 2, motion 2, density 3 on the
court page and 6 on the consoles (occasional use, never all-day). Scene: a sweaty player, one hand, bright court or hall
lights, seconds of attention; staff at a front-desk laptop at opening and closing.

## Colors

Restrained: three neutrals (card `#f6f4ee`, surface `#fffdf7`, container `#ece8dd`), **ink** (`#1b1b1b`) for text,
rules, focus and links, and **highlighter** (`#ffe14d`) only as the fill of the primary action and the current
selection. **The Highlighter Rule.** Highlighter never carries text colour or meaning on its own; it always sits inside
an ink border with ink text on it (13:1), so the action stays visible in glare and under every colour-vision profile.
Status uses red, green and amber *with an icon and words* (status-cue: icon), never colour alone. Light theme only: the
sign is paper; a dark sign does not exist.

## Typography

One workhorse family from the device (Arial / Helvetica / the phone's sans-serif), so the box ships no font files and
works offline. Court names and the action label are heavy (800) and large; body 16 px; times and counts use tabular
numerals. **The Sign Rule.** Uppercase only for the court name on the court page header, nowhere else.

## Layout

Phone first (375–430 px): a single column, 16 px gutter, sections stacked in the agreed order (A-112): venue logo + court
name + recording status → Clip that → game panel → "Send our videos to" → clips on this court. Consoles: top bar with
the venue and box health, left navigation (Courts, Clips, Footage, Staff, Settings, Help) collapsing to a menu below
768 px; tables scroll inside a labelled region, never the page. Breakpoints 375 / 768 / 1024 / 1440. Spacing rhythm 4 px
base; sections 16 px apart on the phone, 24 px on the consoles.

## Elevation & Depth

Flat. Depth is a 2 px ink border (3 px on the primary action); no shadows, no gradients, no glow.

## Shapes

One radius: 4 px on buttons, panels and inputs. The only exception is the email chips (pill), which read as removable
tokens. Borders: 2 px ink on panels and inputs; 1 px `#8b877c` for table row dividers.

## Imagery

Real camera stills (clip thumbnails; the live preview in setup) and the venue logo (court page header, emails, signs,
burned into clips). Two authored black-line drawings in the sign style: the setup wizard (court with the camera mounted
high at the centre and the sign on the net post) and the empty clip library (a phone scanning a court code). Inventory
and alt texts in `docs/requirements/assets/manifest.json`. Omitted on purpose: decorative art on any task screen.

## Components

Buttons: primary = highlighter fill, 3 px ink border, ink text, 48 px tall; secondary = surface with 2 px ink border;
danger = surface with red text and border. Inputs: 2 px ink border, 44 px tall, label always visible above. Tables: real
`<table>` with caption and scoped headers, 1 px row dividers, tabular numbers right-aligned. Chips: pill, 2 px ink
border, a "Remove" button inside. Every interactive component has default, hover (container tint), focus (3 px ink
outline offset 2 px), active, disabled (variant ink, no pointer), loading (text "Saving…"), error and success states.

## Motion

Exactly one moment: after **Clip that**, a ring fills over the clip's remaining window (up to 15 s, linear, because it
measures time) with "Saving… ready in N s", and the phone vibrates once where the browser allows. State changes use
150–200 ms opacity/colour transitions with ease-out; nothing moves on page load. **prefers-reduced-motion:** the ring is
static, the countdown text still updates each second, and all transitions are removed.

## States

Loading: text placeholders ("Saving… ready in 4 s", "Highlights coming — about 25 min"), never a spinner inside content.
Empty: the empty clip library shows the line drawing and "No clips yet — players tap Clip that on the court page";
first-use differs from no-results ("No clips match these filters" with Clear filters). Error: what failed, why, what to
do ("Court 2 camera is offline — clips aren't available"; "These videos have expired"). Success: plain confirmation
("Kept until 21 Oct 2027"). Unreachable box: "Box unreachable since 7:02 pm" on its courts.

## Navigation

| Route | Archetype | Object | Roles | Placement | Primary action | SC | Cross-links |
|---|---|---|---|---|---|---|---|
| `/c/:code` | pos/kiosk | Court page | player | global | Clip that | SC-1, SC-2, SC-3, SC-10 | /c/:code/clips/:id |
| `/c/:code/clips/:id` | record | Clip | player | contextual | Download | SC-1, SC-3 | /c/:code |
| `/setup` | onboarding | Venue | owner | global | Continue | SC-4 | /console |
| `/console/signin` | auth | Staff account | owner, staff | global | Sign in | SC-8 | /console |
| `/console` | dashboard | Box health | owner, staff | global | Open court | SC-5, SC-14 | /console/courts/:id |
| `/console/courts` | list+crud | Court | owner | global | Add court | SC-4, SC-5 | /console/courts/:id |
| `/console/courts/:id` | form | Court | owner | contextual | Save court | SC-4 | /console/courts |
| `/console/clips` | list+crud | Clip | owner, staff | global | Keep | SC-6, SC-7 | /c/:code/clips/:id |
| `/console/footage` | form | Footage hold | owner, staff | global | Hold footage | SC-7 | /console/log |
| `/console/staff` | list+crud | Staff account | owner | global | Add staff | SC-8 | /console/staff/:id |
| `/console/staff/:id` | form | Staff account | owner | contextual | Save | SC-8 | /console/staff |
| `/console/settings` | settings | Venue settings | owner | global | Save settings | SC-4, SC-9, SC-11 | /console/signs |
| `/console/account` | settings | Staff account | owner, staff | global | Change password | SC-8 | /console |
| `/console/signs` | report | Court sign | owner, staff | contextual | Print signs | SC-4 | /console/courts |
| `/console/log` | list+crud | Activity entry | owner | global | Export CSV | SC-6, SC-7 | /console/clips |
| `/console/help` | docs | Help | owner, staff | global | Call support | SC-9 | /console |
| `/v/:token` | record | Take-home email videos | player | global | Download | SC-3, SC-12 | /never/:token |
| `/never/:token` | form | Blocked address | player | contextual | Don't email me again | SC-3 | /v/:token |
| `/fleet/signin` | auth | Fleet user | admin, installer | global | Sign in | SC-13 | /fleet |
| `/fleet` | dashboard | Box health | admin, installer | global | Open venue | SC-9, SC-14 | /fleet/venues/:id |
| `/fleet/venues` | list+crud | Venue | admin | global | Add venue | SC-4, SC-13 | /fleet/venues/:id |
| `/fleet/venues/:id` | record | Venue | admin, installer | contextual | Switch online features | SC-11, SC-13 | /fleet/updates |
| `/fleet/updates` | list+crud | Update release | admin | global | Publish update | SC-9 | /fleet |
| `/fleet/users` | list+crud | Fleet user | admin | global | Add user | SC-13 | /fleet/log |
| `/fleet/log` | list+crud | Fleet log entry | admin | global | Filter | SC-11, SC-13 | /fleet |

## Voice & terms

Plain, short, second person, sentence case, no exclamation marks; ranges use an en dash. Say what happened and what to
do. Tone: errors calm and specific; success brief; empty states explain how to start; destructive actions name the item
and the consequence ("Delete this clip? It can't be recovered."); waiting states give a number of seconds or minutes.
Terms come from the SRS glossary (frontmatter `terms:`): court page, court code, Clip that, keep, hold, game, highlights.
Staff sign-in expiry reads "Please sign in again", never "session".

## Do's and Don'ts

Do: keep one highlighter action per state; pair every status colour with an icon and words; put the venue logo at the
top of the court page; label every input; show times in Manila time with "am/pm"; make tables real tables.
Don't: add shadows, gradients, glows or a second accent; animate anything except the saving ring; use emoji as icons;
put a kicker label above headings; nest panels inside panels; open a modal where an inline confirmation works; use
"session", "admin panel" or "NVR" in any label.

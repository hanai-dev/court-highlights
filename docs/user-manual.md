# Court Highlights — user guide

Court Highlights records every court at your venue with one camera per court. Players save the moment they just
played with one tap, on their own phones, with no app and no account. Everything works on the venue's own network,
with no internet: players download on court, and nothing leaves the venue.

This guide has three parts: **players**, **venue staff** and **the venue Owner**.

## Players

1. **Scan the court code** on the sign at the net post. Your phone must be on the venue Wi-Fi. The court page opens.
2. **Clip that**: tap it right after a good point. The clip covers the 25 seconds before your tap and 5 seconds
   after. Tap again within 5 seconds to make it longer. The ring fills while the clip saves (about 10 seconds), then the
   clip appears under **Recent clips**.
3. **Download** a clip in two shapes: wide for watching, vertical for stories and reels. Clips stay on the court page
   for 2 hours.
4. **Hide** a clip you don't want others on the court to see; it disappears from the court page at once.
5. **Games**: tap **Start game** before you play and **Stop game** when you finish (a game stops by itself after
   2 hours, or at closing time). You get the whole game video and up to 5 highlights. Tick **The venue may post our
   highlights on its pages** only if everyone on court agrees.
6. **Take them home**: download before you leave the venue. Clips and game videos stay on the court page for
   2 hours.

No sound is ever recorded.

## Venue staff

Open the staff console at the address on your staff card (it starts with `https://`). The first time, your browser
warns about the box's own certificate: compare the fingerprint with the one on the card, then continue.

| Task | Where |
|---|---|
| Is every court recording? | **Courts today**: each court's state, last clip, and each box's disk, temperature and reachability |
| Find a clip a player asks about | **Clips**: filter by court, date, type, kept, venue may post |
| Keep a clip for up to 1 year | **Clips** → **Keep** (otherwise clips expire after 72 hours) |
| Download for the venue's pages | **Clips** → **For posting**. If the players did not allow posting you are warned, and the choice is logged |
| Delete a clip a player wants gone | **Clips** → **Delete**, confirm |
| Keep footage for an incident | **Footage** → court, date, time → **Find footage** → **Hold footage** (up to 60 days). Footage is otherwise overwritten after 4 hours |
| Footage that is already gone | **Footage** → **Record that it no longer exists**, so the request is answered on record |
| Print court signs | **Courts** → **Print court signs**; one sign per court |
| Change your password | **My account** |

Every action is in the **Activity log** with who did it and when; the Owner can export it as CSV.

## The venue Owner

### First start

A new box opens the first-run setup at its staff address. It has no default account.

1. **Owner and venue**: venue name, logo (PNG or JPEG, up to 2 MB), opening hours, your name, username and
   password (at least 12 characters). Replacing a box? Use **Restore from backup** on the same page instead.
2. **Courts and cameras**: cameras on the venue network are listed. Pick one, enter the camera's username and
   password, name the court, **Add court**. A camera that isn't listed can be added by its stream address
   (`rtsp://…`). Each court shows a preview once it records.
3. **Court signs**: print them and hang one at each net post. **Finish setup** opens the clip library.

### Settings

| Setting | What it does |
|---|---|
| Venue name, logo, logo corner, sponsor logo | Shown on court pages and signs, and burned into every clip |
| Opening hours | Courts record only during these hours |
| Seconds before / after the tap | Clip length (default 25 + 5) |
| Make highlights | Turns the shot judge on or off |
| Privacy notice and contact | The notice players see from the court page |
| Correct the time | Sets the time on every box at the venue |
| Settings backup | An encrypted file of courts, cameras, logo, settings and staff accounts; keep it safe |
| Software update | Shows the box's version and its last update. Install an update file from a USB stick here, on each box of the venue; a version that doesn't work is taken back by the box itself ("Update rolled back") |
| Wipe this box | For a venue that closes: erases all footage, clips, games, accounts and logs. Type the venue name to confirm |

### Staff

**Staff** → **Add staff**: you get a temporary password to hand over; the person sets their own at first sign-in.
**Reset password** issues a new temporary one. **Deactivate** ends their access on every box within a minute. Five
wrong passwords lock an account for 15 minutes.

### Several boxes

A box serves about 4 courts. At a bigger venue every box shows the whole venue: all courts on **Courts today**, all clips in the
library. If one box is off, its courts show as unreachable and everything else keeps working. A replacement box
fills itself in from the others within a minute.

## What changes at the venue

| Before | With Court Highlights |
|---|---|
| Players prop a phone against the fence and scroll through a long video afterwards | Tap **Clip that** after the point; the clip is on the phone about 10 seconds later |
| A player asks the front desk to send them a video | The player downloads it on court before leaving; the front desk does nothing |
| Staff search a security recorder for an incident hours later | **Footage** → find → **Hold**; the Owner exports it |
| The venue asks players to send videos it may post | **Clips** → filter **Venue may post** → **For posting** |

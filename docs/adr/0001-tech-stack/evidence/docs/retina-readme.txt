https://github.com/scottlamb/retina
accessed 2026-10-20T15:35:25.122Z
HTTP 200

Skip to content
Navigation Menu
Platform
Solutions
Resources
Open Source
Enterprise
Pricing
Sign in
Sign up
scottlamb
retina
Repository navigation
Code
Issues
19
 (19)
Pull requests
12
 (12)
Discussions
Actions
Projects
Security and quality
Insights
retina
Public
Notifications
Fork90
 (90)
Star377
 (377)
main
3 Branches
39 Tags
Code
Latest commit
scottlamb
fix receiver_reports_tcp flake under paused time
6c0f259
 · 
History
388 Commits
Folders and files
Name	Last commit message	Last commit date

.github
	
upgrade some github ci actions
	


benches
	
fix clippy::useless_borrows_in_formatting warnings
	


examples
	
fix clippy::useless_borrows_in_formatting warnings
	


fuzz
	
avoid copying packets on Demuxed path
	


src
	
fix receiver_reports_tcp flake under paused time
	


.gitignore
	
initial commit
	


AUTHORS
	
release v0.4.21
	


CHANGELOG.md
	
release v0.4.21
	


Cargo.lock
	
Upgrade to sdp-types 0.2 to fix a panic on malformed SDP
	


Cargo.toml
	
Upgrade to sdp-types 0.2 to fix a panic on malformed SDP
	


LICENSE-APACHE.txt
	
initial commit
	


LICENSE-MIT.txt
	
initial commit
	


README.md
	
use own RTSP/1.0 parser
	


clippy.toml
	
interpret Annex B sequences
	
Repository files navigation
README
Apache-2.0 license
MIT license
retina

  

High-level RTSP multimedia streaming library, in Rust. Good support for ONVIF RTSP/1.0 IP surveillance cameras, as needed by Moonfire NVR. Works around brokenness in cheap closed-source cameras.

Status: In production use in Moonfire NVR. Many missing features.

Progress:

 client support
 basic authentication.
 digest authentication.
 RTP over TCP via RTSP interleaved channels.
 RTP over UDP (experimental).
 re-order buffer. (Out-of-order packets are dropped now.)
 RTSP/1.0.
 RTSP/2.0.
 SRTP.
 ONVIF backchannel support (for sending audio).
 ONVIF replay mode.
 receiving RTCP Sender Reports (currently only uses the timestamp)
 sending RTCP Receiver Reports
 server support
I/O modes
 async with tokio
 async-std
 synchronous with std only
codec depacketization
 video
 H.264 (RFC 6184](https://datatracker.ietf.org/doc/html/rfc6184))
 SVC
 periodic infra refresh
 multiple slices per picture
 multiple SPS/PPS
 interleaved mode
 AAC output format
 Annex B output format (#44)
 H.265 (RFC 7798)
 MJPEG (RFC 2435)
audio
 AAC
 interleaving
 RFC 3551 codecs: G.711, G.723, L8/L16
 application: ONVIF metadata
 clean, stable API. (See #47.)
quality errors
 detailed error description text.
 programmatically inspectable error type.
 good functional testing coverage. (Currently lightly / unevenly tested. Most depacketizers have no tests.)
 fuzz testing. (In progress.)
 benchmark

Help welcome!

Getting started

Try the mp4 example. It streams from an RTSP server to a .mp4 file until you hit ctrl-C.

$ cargo run --package client mp4 -- --url rtsp://ip.address.goes.here/ --username admin --password test out.mp4
...
^C
Example client
$ cargo run --package client <CMD>

Where CMD:

info - Gets info about available streams and exits.
jpeg - Writes depacketized JPEG images to disk; exit with CTRL+C.
mp4 - Writes RTSP streams to mp4 file; exit with Ctrl+C.
onvif - Gets realtime onvif metadata if available; exit with Ctrl+C.
webcodecs - Decodes video frames using WebCodecs API; exit with Ctrl+C. This is the absolute lowest-latency way to watch RTSP streams from a browser!

The client also respects the common RUST_LOG environment variable: RUST_LOG=client=debug,retina=debug,info will enable debug logging for the client and retina crates, leaving other Rust crates at info level.

Example WebRTC proxy

This allows viewing a H.264 video stream from your browser, with the help of webrtc-rs.

$ cargo run --package webrtc-proxy -- --help
Example ffmpeg decode tool

This decodes video from an RTSP source into raw video frames in PPM format using the ffmpeg-next crate.

Acknowledgements

This builds on the whole Rust ecosystem. A couple folks have been especially helpful:

Sebastian Dröge, author of sdp-types
David Holroyd, author of h264-reader
Why "retina"?

It's a working name. Other ideas welcome. I started by looking at dictionary words with the letters R, T, S, and P in order and picking out ones related to video:

$ egrep '^r.*t.*s.*p' /usr/share/dict/words'	
retinoscope	close but too long, thus retina
retrospect	good name for an NVR, but I already picked Moonfire
rotascope	misspelling of "rotascope" (animation tool) or archaic name for "gyroscope"?
License

Your choice of MIT or Apache; see LICENSE-MIT.txt or LICENSE-APACHE, respectively.

About

High-level RTSP multimedia streaming library, in Rust

crates.io/crates/retina
Topics
ip-camera
rtsp
rust
Resources
Readme
Apache-2.0, MIT licenses found
Activity
Stars
377 stars
Watchers
6 watching
Forks
90 forks
Report repository
Releases
39tags
Used by
230
 (230)
+ 225
Contributors
16
 (16)
+ 2 contributors
Languages
Rust
100%
Footer
© 2026 GitHub, Inc.
Footer navigation
Terms
Privacy
Security
Status
Community
Docs
Contact
Manage cookies
Do not share my personal information

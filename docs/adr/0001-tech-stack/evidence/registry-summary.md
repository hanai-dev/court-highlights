# Registry / advisory / EOL / repo sweep — accessed 2026-10-20

## crates.io + deps.dev (cargo)

| crate | latest stable | released | licence (crates.io) | deps.dev licences | advisories (deps.dev) | repo |
|---|---|---|---|---|---|---|
| tokio | 1.54.0 | 2026-10-19 | MIT | MIT | 0 | https://github.com/tokio-rs/tokio |
| axum | 0.8.9 | 2026-04-14 | MIT | MIT | 0 | https://github.com/tokio-rs/axum |
| retina | 0.4.21 | 2026-10-04 | MIT/Apache-2.0 | Apache-2.0 OR MIT | 0 | https://github.com/scottlamb/retina |
| gstreamer | 0.25.4 | 2026-09-21 | MIT OR Apache-2.0 | Apache-2.0 OR MIT | 0 | https://gitlab.freedesktop.org/gstreamer/gstreamer-rs |
| gstreamer-video | 0.25.4 | 2026-09-21 | MIT OR Apache-2.0 | Apache-2.0 OR MIT | 0 | https://gitlab.freedesktop.org/gstreamer/gstreamer-rs |
| ort | 2.0.0-rc.15 | 2026-10-19 | MIT OR Apache-2.0 | Apache-2.0 OR MIT | 0 | https://github.com/pykeio/ort |
| rusqlite | 0.40.2 | 2026-08-08 | MIT | MIT | 0 | https://github.com/rusqlite/rusqlite |
| askama | 0.16.1 | 2026-09-04 | MIT OR Apache-2.0 | Apache-2.0 OR MIT | 0 | https://github.com/askama-rs/askama |
| argon2 | 0.6.0 | 2026-08-27 | MIT OR Apache-2.0 | Apache-2.0 OR MIT | 0 | https://github.com/RustCrypto/password-hashes |
| totp-rs | 6.0.0 | 2026-08-06 | MIT | MIT | 0 | https://github.com/constantoine/totp-rs |
| aws-sdk-s3 | 1.152.0 | 2026-10-01 | Apache-2.0 | Apache-2.0 | 0 | https://github.com/awslabs/aws-sdk-rust |
| aws-sdk-sesv2 | 1.140.1 | 2026-10-08 | Apache-2.0 | Apache-2.0 | 0 | https://github.com/awslabs/aws-sdk-rust |
| sqlx | 0.9.0 | 2026-05-21 | MIT OR Apache-2.0 | Apache-2.0 OR MIT | 0 | https://github.com/launchbadge/sqlx |
| tower-http | 0.7.1 | 2026-08-31 | MIT | MIT | 0 | https://github.com/tower-rs/tower-http |

## PyPI + deps.dev (pypi)

| package | latest | requires_python | licence (PyPI) | deps.dev licences | advisories |
|---|---|---|---|---|---|
| fastapi | 0.153.0 | >=3.10 | MIT | MIT | 0 |
| uvicorn | 0.54.0 | >=3.10 | BSD-3-Clause | BSD-3-Clause | 0 |
| openvino | 2026.4.1 | >=3.10 | Apache-2.0 | Apache-2.0 | 0 |
| onnxruntime | 1.31.0 | >=3.11 | MIT License | MIT | 0 |
| peewee | 4.5.3 | >=3.8 | (classifier only) |  | 0 |

## OSV querybatch (pinned latest versions)

- crates.io tokio@1.54.0: 0 vulnerabilities
- crates.io axum@0.8.9: 0 vulnerabilities
- crates.io retina@0.4.21: 0 vulnerabilities
- crates.io gstreamer@0.25.4: 0 vulnerabilities
- crates.io gstreamer-video@0.25.4: 0 vulnerabilities
- crates.io ort@2.0.0-rc.15: 0 vulnerabilities
- crates.io rusqlite@0.40.2: 0 vulnerabilities
- crates.io askama@0.16.1: 0 vulnerabilities
- crates.io argon2@0.6.0: 0 vulnerabilities
- crates.io totp-rs@6.0.0: 0 vulnerabilities
- crates.io aws-sdk-s3@1.152.0: 0 vulnerabilities
- crates.io aws-sdk-sesv2@1.140.1: 0 vulnerabilities
- crates.io sqlx@0.9.0: 0 vulnerabilities
- crates.io tower-http@0.7.1: 0 vulnerabilities
- PyPI fastapi@0.153.0: 0 vulnerabilities
- PyPI uvicorn@0.54.0: 0 vulnerabilities
- PyPI openvino@2026.4.1: 0 vulnerabilities
- PyPI onnxruntime@1.31.0: 0 vulnerabilities
- PyPI peewee@4.5.3: 0 vulnerabilities

## endoflife.date

- python: 3.15 (released 2026-10-09, eoas 2028-10-09, eol 2031-10-31); 3.14 (released 2025-10-07, eoas 2027-10-01, eol 2030-10-31); 3.13 (released 2024-10-07, eoas 2026-10-01, eol 2029-10-31); 3.12 (released 2023-10-02, eoas 2025-04-02, eol 2028-10-31); 3.11 (released 2022-10-24, eoas 2024-04-01, eol 2027-10-31); 3.10 (released 2021-10-04, eoas 2023-04-05, eol 2026-10-01)
- ubuntu: 26.10 (released 2026-10-15, eoas 2027-07-01, eol 2027-07-01); 26.04 (released 2026-04-23, eoas 2031-05-29, eol 2031-05-29, LTS); 25.10 (released 2025-10-09, eoas 2026-07-01, eol 2026-07-01); 25.04 (released 2025-04-17, eoas 2026-01-17, eol 2026-01-17); 24.10 (released 2024-10-10, eoas 2025-07-10, eol 2025-07-10); 24.04 (released 2024-04-25, eoas 2029-05-31, eol 2029-05-31, LTS)
- postgresql: 18 (released 2025-09-25, eoas -, eol 2030-11-14); 17 (released 2024-09-26, eoas -, eol 2029-11-08); 16 (released 2023-09-14, eoas -, eol 2028-11-09); 15 (released 2022-10-13, eoas -, eol 2027-11-11); 14 (released 2021-09-30, eoas -, eol 2026-11-12); 13 (released 2020-09-24, eoas -, eol 2025-11-13)
- rust: 1.99 (released 2026-10-01, eoas -, eol -); 1.98 (released 2026-08-20, eoas -, eol 2026-10-01); 1.97 (released 2026-07-09, eoas -, eol 2026-08-20); 1.96 (released 2026-05-28, eoas -, eol 2026-07-09); 1.95 (released 2026-04-16, eoas -, eol 2026-05-28); 1.94 (released 2026-03-06, eoas -, eol 2026-04-16)
- nvidia-jetson: HTTP 404 (no product)
- jetpack: HTTP 404 (no product)
- openvino: HTTP 404 (no product)
- ffmpeg: 9.0 (released 2026-08-04, eoas -, eol -); 8.1 (released 2026-03-16, eoas -, eol -); 8.0 (released 2025-08-22, eoas -, eol -); 7.1 (released 2024-09-30, eoas -, eol -, LTS); 7.0 (released 2024-04-05, eoas -, eol 2026-06-22); 6.1 (released 2023-11-11, eoas -, eol -)
- debian: 13 (released 2025-08-09, eoas 2028-08-09, eol 2030-06-30); 12 (released 2023-06-10, eoas 2026-07-11, eol 2028-06-30); 11 (released 2021-08-14, eoas 2024-08-14, eol 2026-08-31); 10 (released 2019-07-06, eoas 2022-09-10, eol 2024-06-30); 9 (released 2017-06-17, eoas 2020-07-18, eol 2022-07-01); 8 (released 2015-04-25, eoas 2018-06-17, eol 2020-06-30)

## GitHub repository metadata

| repo | licence | archived | last push | stars | open issues |
|---|---|---|---|---|---|
| blakeblackshear/frigate | MIT | false | 2026-10-20 | 36772 | 94 |
| scottlamb/retina | Apache-2.0 | false | 2026-10-19 | 377 | 31 |
| pykeio/ort | Apache-2.0 | false | 2026-10-19 | 2576 | 3 |
| openvinotoolkit/openvino | Apache-2.0 | false | 2026-10-20 | 11029 | 882 |
| microsoft/onnxruntime | MIT | false | 2026-10-20 | 22121 | 1819 |
| FFmpeg/FFmpeg | NOASSERTION | false | 2026-10-20 | 65468 | 3 |
| GStreamer/gstreamer | NOASSERTION | false | 2026-10-20 | 3345 | 14 |
| rust-lang/rust | Apache-2.0 | false | 2026-10-20 | 120650 | 12620 |
| tokio-rs/axum | MIT | false | 2026-10-20 | 27542 | 111 |
| ultralytics/ultralytics | AGPL-3.0 | false | 2026-10-20 | 62670 | 89 |

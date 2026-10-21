//! GStreamer jobs: camera stills and the clip cutter (FR-4, FR-6, FR-7). Decode and encode use the N150's VA
//! hardware when present and OpenH264 otherwise (dev containers, CI). No GPL elements are ever chosen (A-114).

use crate::web::AppError;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::path::{Path, PathBuf};

fn fail(e: impl std::fmt::Display) -> AppError {
    AppError::Internal(format!("media: {e}"))
}

fn launch(description: &str) -> Result<gst::Pipeline, AppError> {
    gst::parse::launch(description).map_err(fail)?.downcast::<gst::Pipeline>().map_err(|_| fail("not a pipeline"))
}

fn element(p: &gst::Pipeline, name: &str) -> Result<gst::Element, AppError> {
    p.by_name(name).ok_or_else(|| fail(format!("no element {name}")))
}

/// Runs a pipeline to EOS; always leaves it stopped.
fn run_to_eos(p: &gst::Pipeline, timeout_s: u64) -> Result<(), AppError> {
    p.set_state(gst::State::Playing).map_err(fail)?;
    let result = wait_eos(p, timeout_s);
    let _ = p.set_state(gst::State::Null);
    result
}

fn wait_eos(p: &gst::Pipeline, timeout_s: u64) -> Result<(), AppError> {
    let bus = p.bus().ok_or_else(|| fail("no bus"))?;
    let msg = bus
        .timed_pop_filtered(gst::ClockTime::from_seconds(timeout_s), &[gst::MessageType::Eos, gst::MessageType::Error])
        .ok_or_else(|| fail("timed out"))?;
    match msg.view() {
        gst::MessageView::Error(e) => Err(fail(format!("{} ({:?})", e.error(), e.debug()))),
        _ => Ok(()),
    }
}

fn first_available(names: &[&'static str]) -> Option<&'static str> {
    names.iter().copied().find(|n| gst::ElementFactory::find(n).is_some())
}

/// One JPEG still from the first frame of an MP4 (camera preview, clip thumbnails).
pub fn still_jpeg(path: &str, width: u32, height: u32) -> Result<Vec<u8>, AppError> {
    let decoder = first_available(&["vah264dec", "openh264dec"]).ok_or_else(|| fail("no H.264 decoder"))?;
    let p = launch(&format!(
        "filesrc name=src ! qtdemux ! h264parse ! {decoder} ! videoconvert ! videoscale \
         ! video/x-raw,width={width},height={height},pixel-aspect-ratio=1/1 ! jpegenc ! appsink name=sink max-buffers=1 sync=false"
    ))?;
    // Paths are set as properties, never pasted into the pipeline description.
    element(&p, "src")?.set_property("location", path);
    let sink = element(&p, "sink")?.downcast::<gst_app::AppSink>().map_err(|_| fail("no sink"))?;
    p.set_state(gst::State::Playing).map_err(fail)?;
    let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(5));
    let _ = p.set_state(gst::State::Null);
    let buffer = sample.as_ref().and_then(|s| s.buffer()).ok_or_else(|| fail("no frame decoded"))?;
    let map = buffer.map_readable().map_err(fail)?;
    Ok(map.as_slice().to_vec())
}

/// Width and height of a PNG or JPEG (the formats the logo upload accepts).
pub fn image_size(b: &[u8]) -> Option<(u32, u32)> {
    if b.starts_with(b"\x89PNG\r\n\x1a\n") && b.len() >= 24 {
        let be = |i: usize| u32::from_be_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        return Some((be(16), be(20)));
    }
    // JPEG: walk the marker segments to the first start-of-frame.
    let mut i = 2;
    while b.starts_with(&[0xFF, 0xD8]) && i + 9 < b.len() && b[i] == 0xFF {
        let marker = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if matches!(marker, 0xC0..=0xCF) && !matches!(marker, 0xC4 | 0xC8 | 0xCC) {
            let h = u16::from_be_bytes([b[i + 5], b[i + 6]]) as u32;
            let w = u16::from_be_bytes([b[i + 7], b[i + 8]]) as u32;
            return Some((w, h));
        }
        i += 2 + len;
    }
    None
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub fn parse(s: &str) -> Self {
        match s {
            "top-left" => Self::TopLeft,
            "bottom-left" => Self::BottomLeft,
            "bottom-right" => Self::BottomRight,
            _ => Self::TopRight,
        }
    }

    /// The other corner on the same edge — where the sponsor logo goes.
    fn mirrored(self) -> Self {
        match self {
            Self::TopLeft => Self::TopRight,
            Self::TopRight => Self::TopLeft,
            Self::BottomLeft => Self::BottomRight,
            Self::BottomRight => Self::BottomLeft,
        }
    }
}

pub struct Logo {
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
}

pub struct Brand {
    pub corner: Corner,
    pub logo: Option<Logo>,
    pub sponsor: Option<Logo>,
}

/// A frame size; outputs are always even-sized for the encoders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Size {
    pub w: u32,
    pub h: u32,
}

fn even(x: f64) -> u32 {
    ((x / 2.0).round() as u32 * 2).max(2)
}

/// Wide keeps the camera's frame, scaled down to fit 1920×1080. Vertical is the middle 9:16 of the frame, as tall
/// as 16/9 of the wide height (1080 → 1080×1920), capped at 1920.
pub fn output_sizes(src: Size) -> (Size, Size) {
    let scale = (1920.0 / src.w as f64).min(1080.0 / src.h as f64).min(1.0);
    let wide = Size { w: even(src.w as f64 * scale), h: even(src.h as f64 * scale) };
    let vh = even((wide.h as f64 * 16.0 / 9.0).min(1920.0));
    (wide, Size { w: even(vh as f64 * 9.0 / 16.0), h: vh })
}

/// Logo box: fits inside a share of the frame, keeps its aspect, sits a 3 % margin from the corner.
fn place(logo: &Logo, frame: Size, corner: Corner) -> (i32, i32, u32, u32) {
    let (max_w, max_h) = if frame.w >= frame.h { (0.18, 0.14) } else { (0.36, 0.09) };
    let k = (frame.w as f64 * max_w / logo.width as f64).min(frame.h as f64 * max_h / logo.height as f64);
    let (w, h) = (even(logo.width as f64 * k), even(logo.height as f64 * k));
    let m = (frame.h.min(frame.w) as f64 * 0.03).round().max(1.0) as i32;
    let (x, y) = match corner {
        Corner::TopLeft => (m, m),
        Corner::TopRight => (-m, m),
        Corner::BottomLeft => (m, -m),
        Corner::BottomRight => (-m, -m),
    };
    (x, y, w, h)
}

fn overlays(brand: &Brand, prefix: &str) -> String {
    [&brand.logo, &brand.sponsor]
        .iter()
        .enumerate()
        .filter(|(_, l)| l.is_some())
        .map(|(i, _)| format!("gdkpixbufoverlay name={prefix}{i} ! "))
        .collect()
}

fn configure_overlays(p: &gst::Pipeline, brand: &Brand, prefix: &str, frame: Size) -> Result<(), AppError> {
    for (i, (logo, corner)) in [(&brand.logo, brand.corner), (&brand.sponsor, brand.corner.mirrored())].into_iter().enumerate() {
        if let Some(logo) = logo {
            let (x, y, w, h) = place(logo, frame, corner);
            let o = element(p, &format!("{prefix}{i}"))?;
            o.set_property("location", logo.path.to_string_lossy().as_ref());
            o.set_property("offset-x", x);
            o.set_property("offset-y", y);
            o.set_property("overlay-width", w as i32);
            o.set_property("overlay-height", h as i32);
        }
    }
    Ok(())
}

pub struct Cut<'a> {
    /// Contiguous recorded segments, oldest first.
    pub segments: Vec<String>,
    /// Window inside the joined segments, in milliseconds from the start of the first one.
    pub from_ms: u64,
    pub to_ms: u64,
    pub source: Size,
    pub brand: &'a Brand,
    pub wide: &'a Path,
    pub vertical: &'a Path,
}

/// Decodes the window once and encodes both shapes with the logos burned in. No audio track is ever written.
pub fn cut(job: &Cut) -> Result<(), AppError> {
    let decoder = first_available(&["vah264dec", "openh264dec"]).ok_or_else(|| fail("no H.264 decoder"))?;
    let encoder = first_available(&["vah264lpenc", "vah264enc", "openh264enc"]).ok_or_else(|| fail("no H.264 encoder"))?;
    let (wide, vertical) = output_sizes(job.source);
    let enc = |name: &str| format!("{encoder} name={name} ! h264parse ! mp4mux faststart=true");
    let p = launch(&format!(
        "splitmuxsrc name=src ! h264parse ! {decoder} ! videoconvert ! tee name=t \
         t. ! queue ! videoscale ! video/x-raw,width={ww},height={wh},pixel-aspect-ratio=1/1 ! {wo}videoconvert ! {we} ! filesink name=wide_out \
         t. ! queue ! aspectratiocrop aspect-ratio=9/16 ! videoscale ! video/x-raw,width={vw},height={vh},pixel-aspect-ratio=1/1 ! {vo}videoconvert ! {ve} ! filesink name=vertical_out",
        ww = wide.w,
        wh = wide.h,
        vw = vertical.w,
        vh = vertical.h,
        wo = overlays(job.brand, "wide_logo"),
        vo = overlays(job.brand, "vertical_logo"),
        we = enc("wide_enc"),
        ve = enc("vertical_enc"),
    ))?;
    let files = job.segments.clone();
    element(&p, "src")?.connect("format-location", false, move |_| Some(files.to_value()));
    element(&p, "wide_out")?.set_property("location", job.wide.to_string_lossy().as_ref());
    element(&p, "vertical_out")?.set_property("location", job.vertical.to_string_lossy().as_ref());
    configure_overlays(&p, job.brand, "wide_logo", wide)?;
    configure_overlays(&p, job.brand, "vertical_logo", vertical)?;
    for (name, size) in [("wide_enc", wide), ("vertical_enc", vertical)] {
        let e = element(&p, name)?;
        // ≈ 0.1 bit per pixel per frame at 30 fps; the kbit/s vs bit/s split is the elements' own convention.
        let bps = (size.w as u64 * size.h as u64 * 3).clamp(500_000, 8_000_000);
        if encoder == "openh264enc" {
            e.set_property("bitrate", bps as u32);
            e.set_property_from_str("complexity", "low");
            e.set_property("multi-thread", 4u32);
        } else {
            e.set_property("bitrate", (bps / 1000) as u32);
        }
    }

    p.set_state(gst::State::Paused).map_err(fail)?;
    let (res, _, _) = p.state(gst::ClockTime::from_seconds(10));
    if res.is_err() {
        let _ = p.set_state(gst::State::Null);
        return Err(fail("clip pipeline did not start"));
    }
    let seek = p.seek(
        1.0,
        gst::SeekFlags::FLUSH | gst::SeekFlags::ACCURATE,
        gst::SeekType::Set,
        gst::ClockTime::from_mseconds(job.from_ms),
        gst::SeekType::Set,
        gst::ClockTime::from_mseconds(job.to_ms),
    );
    if let Err(e) = seek {
        let _ = p.set_state(gst::State::Null);
        return Err(fail(e));
    }
    // ponytail: a 30 s window at real-time × 4 worst case; tune once the N150 numbers are in (NFR-1).
    run_to_eos(&p, 120)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_and_logo_geometry() {
        assert_eq!(output_sizes(Size { w: 1920, h: 1080 }), (Size { w: 1920, h: 1080 }, Size { w: 1080, h: 1920 }));
        assert_eq!(output_sizes(Size { w: 3840, h: 2160 }), (Size { w: 1920, h: 1080 }, Size { w: 1080, h: 1920 }));
        assert_eq!(output_sizes(Size { w: 640, h: 360 }), (Size { w: 640, h: 360 }, Size { w: 360, h: 640 }));
        let logo = Logo { path: PathBuf::new(), width: 400, height: 100 };
        // 18 % of 1920 = 345.6 wide → 346×86, 32 px margin, top-right is negative x.
        assert_eq!(place(&logo, Size { w: 1920, h: 1080 }, Corner::TopRight), (-32, 32, 346, 86));
        assert_eq!(place(&logo, Size { w: 1080, h: 1920 }, Corner::BottomLeft), (32, -32, 388, 98));
    }

    #[test]
    fn image_sizes() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend_from_slice(&[0, 0, 1, 144, 0, 0, 0, 100]);
        assert_eq!(image_size(&png), Some((400, 100)));
        // SOI, an APP0 segment of length 4, then SOF0 with height 50 and width 120.
        let jpeg = [0xFF, 0xD8, 0xFF, 0xE0, 0, 4, 0, 0, 0xFF, 0xC0, 0, 11, 8, 0, 50, 0, 120, 3, 0, 0];
        assert_eq!(image_size(&jpeg), Some((120, 50)));
        assert_eq!(image_size(b"GIF89a"), None);
    }
}

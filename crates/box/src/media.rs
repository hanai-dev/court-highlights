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
    let decoder = h264_decoder()?;
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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shape {
    /// The camera's frame, at most 1920×1080.
    Wide,
    /// The middle 9:16 of the frame.
    Vertical,
    /// The camera's frame, at most 1280×720 — the copy of a game video that is emailed (FR-12).
    Email,
}

impl Shape {
    fn size(self, source: Size) -> Size {
        let (wide, vertical) = output_sizes(source);
        match self {
            Self::Wide => wide,
            Self::Vertical => vertical,
            Self::Email => {
                let k = (1280.0 / source.w as f64).min(720.0 / source.h as f64).min(1.0);
                Size { w: even(source.w as f64 * k), h: even(source.h as f64 * k) }
            }
        }
    }
}

pub struct Output<'a> {
    pub shape: Shape,
    pub path: &'a Path,
}

pub struct Cut<'a> {
    /// Recorded segments, oldest first; splitmuxsrc plays them back to back.
    pub segments: Vec<String>,
    /// Window in recorded time from the start of the first segment.
    pub from_ms: u64,
    pub to_ms: u64,
    pub source: Size,
    pub brand: &'a Brand,
    pub outputs: Vec<Output<'a>>,
}

fn h264_encoder() -> Result<&'static str, AppError> {
    first_available(&["vah264lpenc", "vah264enc", "openh264enc"]).ok_or_else(|| fail("no H.264 encoder"))
}

fn h264_decoder() -> Result<&'static str, AppError> {
    first_available(&["vah264dec", "openh264dec"]).ok_or_else(|| fail("no H.264 decoder"))
}

/// ≈ 0.1 bit per pixel per frame at 30 fps; the kbit/s vs bit/s split is the elements' own convention.
fn configure_encoder(e: &gst::Element, encoder: &str, size: Size) {
    let bps = (size.w as u64 * size.h as u64 * 3).clamp(500_000, 8_000_000);
    if encoder == "openh264enc" {
        e.set_property("bitrate", bps as u32);
        e.set_property_from_str("complexity", "low");
        e.set_property("multi-thread", 4u32);
    } else {
        e.set_property("bitrate", (bps / 1000) as u32);
    }
}

fn set_segments(p: &gst::Pipeline, segments: &[String]) -> Result<(), AppError> {
    let files = segments.to_vec();
    element(p, "src")?.connect("format-location", false, move |_| Some(files.to_value()));
    Ok(())
}

/// Decodes the window once and encodes every requested shape with the logos burned in. No audio track is ever
/// written.
pub fn cut(job: &Cut) -> Result<(), AppError> {
    let (decoder, encoder) = (h264_decoder()?, h264_encoder()?);
    let branches: String = job
        .outputs
        .iter()
        .enumerate()
        .map(|(i, o)| {
            let size = o.shape.size(job.source);
            let crop = if o.shape == Shape::Vertical { "aspectratiocrop aspect-ratio=9/16 ! " } else { "" };
            format!(
                " t. ! queue ! {crop}videoscale ! video/x-raw,width={},height={},pixel-aspect-ratio=1/1 ! {}videoconvert ! {encoder} name=enc{i} ! h264parse ! mp4mux faststart=true ! filesink name=out{i}",
                size.w,
                size.h,
                overlays(job.brand, &format!("logo{i}_")),
            )
        })
        .collect();
    let p = launch(&format!("splitmuxsrc name=src ! h264parse ! {decoder} ! videoconvert ! tee name=t{branches}"))?;
    set_segments(&p, &job.segments)?;
    for (i, o) in job.outputs.iter().enumerate() {
        let size = o.shape.size(job.source);
        element(&p, &format!("out{i}"))?.set_property("location", o.path.to_string_lossy().as_ref());
        configure_overlays(&p, job.brand, &format!("logo{i}_"), size)?;
        configure_encoder(&element(&p, &format!("enc{i}"))?, encoder, size);
    }
    trim(&element(&p, "t")?, job.from_ms, job.to_ms)?;
    // ponytail: real-time × 4 worst case for the window; tune once the N150 numbers are in (NFR-1, NFR-3).
    run_to_eos(&p, (job.to_ms.saturating_sub(job.from_ms) / 250).max(120))
}

/// Movement over a window, 5 samples a second: mean absolute difference between consecutive 96×54 grey frames
/// (0–255). Input to the baseline shot judge. Returns (ms from window start, movement).
pub fn motion_profile(segments: &[String], from_ms: u64, to_ms: u64) -> Result<Vec<(u64, f32)>, AppError> {
    let decoder = h264_decoder()?;
    let p = launch(&format!(
        "splitmuxsrc name=src ! h264parse ! {decoder} ! identity name=gate ! videorate drop-only=true ! video/x-raw,framerate=5/1          ! videoscale ! videoconvert ! video/x-raw,format=GRAY8,width=96,height=54 ! appsink name=sink sync=false max-buffers=64"
    ))?;
    set_segments(&p, segments)?;
    trim(&element(&p, "gate")?, from_ms, to_ms)?;
    let sink = element(&p, "sink")?.downcast::<gst_app::AppSink>().map_err(|_| fail("no sink"))?;
    p.set_state(gst::State::Playing).map_err(fail)?;
    let mut out = Vec::new();
    let mut previous: Option<Vec<u8>> = None;
    // Ends at EOS (the trim probe) or if decoding stalls for 30 s.
    while let Some(sample) = sink.try_pull_sample(gst::ClockTime::from_seconds(30)) {
        let Some(buffer) = sample.buffer() else { continue };
        // Running time: 0 at the window start (the trim probe re-bases it); raw PTS starts at 1000 s.
        let segment = sample.segment().and_then(|s| s.downcast_ref::<gst::ClockTime>().cloned());
        let Some(at) = segment.zip(buffer.pts()).and_then(|(s, pts)| s.to_running_time(pts)).map(|t| t.mseconds()) else { continue };
        let frame = buffer.map_readable().map_err(fail)?.as_slice().to_vec();
        if let Some(prev) = &previous
            && prev.len() == frame.len()
        {
            let diff: u64 = prev.iter().zip(&frame).map(|(a, b)| a.abs_diff(*b) as u64).sum();
            out.push((at, diff as f32 / frame.len() as f32));
        }
        previous = Some(frame);
    }
    let ended = p.bus().and_then(|b| b.pop_filtered(&[gst::MessageType::Error]));
    let _ = p.set_state(gst::State::Null);
    if let Some(msg) = ended
        && let gst::MessageView::Error(e) = msg.view()
    {
        return Err(fail(e.error()));
    }
    Ok(out)
}

/// Joins already-encoded files of one shape into one, re-encoded so the result is a single clean stream (reels).
pub fn concat_encode(files: &[PathBuf], size: Size, out: &Path) -> Result<(), AppError> {
    let (decoder, encoder) = (h264_decoder()?, h264_encoder()?);
    let p = launch(&format!(
        "splitmuxsrc name=src ! h264parse ! {decoder} ! videoconvert ! {encoder} name=enc ! h264parse ! mp4mux faststart=true ! filesink name=out"
    ))?;
    let names: Vec<String> = files.iter().map(|f| f.to_string_lossy().into_owned()).collect();
    set_segments(&p, &names)?;
    configure_encoder(&element(&p, "enc")?, encoder, size);
    element(&p, "out")?.set_property("location", out.to_string_lossy().as_ref());
    run_to_eos(&p, 600)
}

/// Keeps only decoded frames whose running time is in [from, to), re-based to start at 0, then ends the stream. No
/// seek: a flushing seek after preroll leaves encoders mid-GOP, so the file would not start on a keyframe. Decoding
/// starts at the first segment's keyframe, at most one segment (10 s) before the window. Running time, not PTS:
/// splitmuxsrc stamps buffers from 1000 s.
fn trim(gate: &gst::Element, from_ms: u64, to_ms: u64) -> Result<(), AppError> {
    let (from, to) = (gst::ClockTime::from_mseconds(from_ms), gst::ClockTime::from_mseconds(to_ms));
    let pad = gate.static_pad("sink").ok_or_else(|| fail("gate has no sink pad"))?;
    let ended = std::sync::atomic::AtomicBool::new(false);
    pad.add_probe(gst::PadProbeType::BUFFER, move |pad, info| {
        let Some(gst::PadProbeData::Buffer(buffer)) = info.data.as_mut() else { return gst::PadProbeReturn::Ok };
        let segment = pad.sticky_event::<gst::event::Segment>(0).and_then(|e| e.segment().downcast_ref::<gst::ClockTime>().cloned());
        let (Some(segment), Some(pts)) = (segment, buffer.pts()) else { return gst::PadProbeReturn::Drop };
        let Some(running) = segment.to_running_time(pts) else { return gst::PadProbeReturn::Drop };
        if running < from {
            return gst::PadProbeReturn::Drop;
        }
        if running >= to || ended.load(std::sync::atomic::Ordering::Relaxed) {
            if !ended.swap(true, std::sync::atomic::Ordering::Relaxed) {
                pad.send_event(gst::event::Eos::new());
            }
            info.data = None;
            info.flow_res = Err(gst::FlowError::Eos);
            return gst::PadProbeReturn::Handled;
        }
        let Some(rebased) = segment.position_from_running_time(running - from) else { return gst::PadProbeReturn::Drop };
        buffer.make_mut().set_pts(rebased);
        gst::PadProbeReturn::Ok
    })
    .ok_or_else(|| fail("could not trim"))?;
    Ok(())
}

/// Joins whole segments into one MP4 by stream copy — no re-encode, so it is quick even for hours of footage.
pub fn export_copy(segments: &[String], out: &Path) -> Result<(), AppError> {
    let p = launch("splitmuxsrc name=src ! h264parse ! mp4mux faststart=true ! filesink name=out")?;
    let files = segments.to_vec();
    element(&p, "src")?.connect("format-location", false, move |_| Some(files.to_value()));
    element(&p, "out")?.set_property("location", out.to_string_lossy().as_ref());
    run_to_eos(&p, 600)
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

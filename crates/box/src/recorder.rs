//! Rolling recording (FR-1…FR-3). Per court: retina reads the camera's RTSP H.264 stream (video only — the audio
//! track is never set up, FR-2), frames go into GStreamer `appsrc ! h264parse ! splitmuxsink`, which writes 10 s
//! MP4 segments by stream copy. Segment files are named by their wall-clock start; their spans live in `segment`.

use crate::{clock, db::Db};
use ch_core::{OpeningHours, Time};
use futures::StreamExt;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::watch;

/// No frame for this long means the camera is gone (FR-3: red within 10 s).
const FRAME_TIMEOUT: Duration = Duration::from_secs(5);
/// Retry quickly so recording resumes within 5 s of the camera returning (RQ-10).
const RETRY_EVERY: Duration = Duration::from_secs(1);
const SEGMENT_NS: u64 = 10_000_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CourtStatus {
    NoCamera,
    Closed,
    Starting,
    Recording { since: Time },
    Offline { since: Time },
}

#[derive(Clone, PartialEq, Eq)]
struct CameraConfig {
    url: String,
    username: String,
    password: String,
}

struct Slot {
    config: CameraConfig,
    stop: watch::Sender<bool>,
    status: Arc<Mutex<CourtStatus>>,
}

#[derive(Clone)]
pub struct Recorders {
    db: Db,
    footage: PathBuf,
    slots: Arc<Mutex<HashMap<i64, Slot>>>,
}

impl Recorders {
    pub fn new(db: Db, footage: PathBuf) -> Self {
        Self { db, footage, slots: Arc::default() }
    }

    pub fn footage_dir(&self) -> &Path {
        &self.footage
    }

    /// Starts and stops court recorders to match the courts, cameras and opening hours; trims rolling footage.
    pub fn spawn_supervisor(&self) {
        let me = self.clone();
        tokio::spawn(async move {
            let mut ticks = 0u64;
            loop {
                if let Err(e) = me.reconcile() {
                    tracing::error!(error = %e, "recorder reconcile failed");
                }
                if ticks % 30 == 0 {
                    if let Err(e) = me.trim_rolling() {
                        tracing::error!(error = %e, "rolling footage trim failed");
                    }
                }
                ticks += 1;
                tokio::time::sleep(Duration::from_secs(2)).await;
            }
        });
    }

    fn hours(&self) -> rusqlite::Result<Option<OpeningHours>> {
        let conn = self.db.conn();
        let row: Option<(String, String)> = rusqlite::OptionalExtension::optional(conn.query_row(
            "SELECT open_time, close_time FROM venue WHERE id = 1",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        ))?;
        Ok(row.and_then(|(o, c)| {
            Some(OpeningHours {
                open: chrono::NaiveTime::parse_from_str(&o, "%H:%M").ok()?,
                close: chrono::NaiveTime::parse_from_str(&c, "%H:%M").ok()?,
            })
        }))
    }

    fn reconcile(&self) -> rusqlite::Result<()> {
        let open = self.hours()?.is_some_and(|h| h.is_open(clock::now()));
        let wanted: HashMap<i64, CameraConfig> = {
            let conn = self.db.conn();
            let mut st = conn.prepare("SELECT id, camera_url, camera_user, camera_password FROM court WHERE camera_url IS NOT NULL")?;
            st.query_map([], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    CameraConfig { url: r.get(1)?, username: r.get::<_, Option<String>>(2)?.unwrap_or_default(), password: r.get::<_, Option<String>>(3)?.unwrap_or_default() },
                ))
            })?
            .collect::<Result<_, _>>()?
        };
        let mut slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        slots.retain(|id, slot| {
            let keep = open && wanted.get(id) == Some(&slot.config);
            if !keep {
                let _ = slot.stop.send(true);
            }
            keep
        });
        if open {
            for (id, config) in wanted {
                if slots.contains_key(&id) {
                    continue;
                }
                let (stop, stopped) = watch::channel(false);
                let status = Arc::new(Mutex::new(CourtStatus::Starting));
                tokio::spawn(run_court(id, config.clone(), self.db.clone(), self.footage.join(format!("court-{id}")), status.clone(), stopped));
                slots.insert(id, Slot { config, stop, status });
            }
        }
        Ok(())
    }

    pub fn status(&self, court_id: i64, has_camera: bool) -> CourtStatus {
        if !has_camera {
            return CourtStatus::NoCamera;
        }
        let slots = self.slots.lock().unwrap_or_else(|p| p.into_inner());
        match slots.get(&court_id) {
            Some(slot) => slot.status.lock().unwrap_or_else(|p| p.into_inner()).clone(),
            None => CourtStatus::Closed,
        }
    }

    /// Rolling footage older than 4 h is deleted unless protected (FR-1, GV-13). Holds are added with FR-32.
    fn trim_rolling(&self) -> rusqlite::Result<()> {
        let now = clock::now();
        let cutoff_ms = ch_core::rolling_cutoff(now).timestamp_millis();
        let doomed: Vec<(i64, String)> = {
            let conn = self.db.conn();
            let mut st = conn.prepare("SELECT id, path FROM segment WHERE end_ms IS NOT NULL AND end_ms <= ?1")?;
            st.query_map([cutoff_ms], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<Result<_, _>>()?
        };
        for (id, path) in doomed {
            if let Err(e) = std::fs::remove_file(&path)
                && e.kind() != std::io::ErrorKind::NotFound
            {
                tracing::warn!(%path, error = %e, "could not delete old footage");
                continue;
            }
            self.db.conn().execute("DELETE FROM segment WHERE id = ?1", [id])?;
        }
        Ok(())
    }
}

fn set(status: &Mutex<CourtStatus>, s: CourtStatus) {
    *status.lock().unwrap_or_else(|p| p.into_inner()) = s;
}

async fn run_court(court: i64, camera: CameraConfig, db: Db, dir: PathBuf, status: Arc<Mutex<CourtStatus>>, mut stop: watch::Receiver<bool>) {
    if let Err(e) = std::fs::create_dir_all(&dir) {
        tracing::error!(court, error = %e, "cannot create footage directory");
        return;
    }
    let mut offline_since: Option<Time> = None;
    loop {
        let inner_stop = stop.clone();
        let result = tokio::select! {
            _ = stop.changed() => break,
            r = record_once(court, &camera, &db, &dir, &status, inner_stop) => r,
        };
        if *stop.borrow() {
            break;
        }
        let was_recording = matches!(*status.lock().unwrap_or_else(|p| p.into_inner()), CourtStatus::Recording { .. });
        if was_recording || offline_since.is_none() {
            offline_since = Some(clock::now());
        }
        set(&status, CourtStatus::Offline { since: offline_since.unwrap_or_else(clock::now) });
        match result {
            Ok(()) => tracing::warn!(court, "camera stream ended"),
            Err(e) => tracing::warn!(court, error = %e, "camera unavailable"),
        }
        tokio::time::sleep(RETRY_EVERY).await;
    }
    tracing::info!(court, "recorder stopped");
}

type BoxError = Box<dyn std::error::Error + Send + Sync>;

async fn record_once(court: i64, camera: &CameraConfig, db: &Db, dir: &Path, status: &Mutex<CourtStatus>, mut stop: watch::Receiver<bool>) -> Result<(), BoxError> {
    use retina::client::{Credentials, PlayOptions, Session, SessionOptions, SetupOptions, Transport};
    let url = url::Url::parse(&camera.url)?;
    let creds = (!camera.username.is_empty()).then(|| Credentials { username: camera.username.clone(), password: camera.password.clone() });
    let mut session = Session::describe(url, SessionOptions::default().creds(creds).user_agent("court-highlights".into())).await?;
    let video = session
        .streams()
        .iter()
        .position(|s| s.media() == "video" && s.encoding_name().eq_ignore_ascii_case("h264"))
        .ok_or("camera offers no H.264 video stream")?;
    session
        .setup(video, SetupOptions::default().transport(Transport::default()).frame_format(retina::codec::FrameFormat::MP4))
        .await?;
    let mut frames = session.play(PlayOptions::default()).await?.demuxed()?;

    let mut writer: Option<SegmentWriter> = None;
    loop {
        let item = tokio::select! {
            _ = stop.changed() => break,
            item = tokio::time::timeout(FRAME_TIMEOUT, frames.next()) => item.map_err(|_| "no frames for 5 s")?,
        };
        let Some(item) = item else { break };
        let retina::codec::CodecItem::VideoFrame(frame) = item? else { continue };
        let params = match frames.streams()[video].parameters() {
            Some(retina::codec::ParametersRef::Video(v)) => v.extra_data().to_vec(),
            _ => continue,
        };
        if writer.as_ref().is_some_and(|w| w.codec_data != params) {
            writer.take().map(|w| w.finish());
        }
        if writer.is_none() {
            if !frame.is_random_access_point() {
                continue; // a segment must start on a keyframe
            }
            writer = Some(SegmentWriter::start(court, db.clone(), dir, params, frame.timestamp().elapsed(), frame.timestamp().clock_rate().get())?);
            set(status, CourtStatus::Recording { since: clock::now() });
        }
        let w = writer.as_mut().expect("writer started above");
        w.push(frame.timestamp().elapsed(), frame.is_random_access_point(), frame.into_data())?;
    }
    if let Some(w) = writer {
        w.finish();
    }
    Ok(())
}

/// One GStreamer pipeline per recording run: appsrc → h264parse → splitmuxsink (stream copy, 10 s fragments).
struct SegmentWriter {
    pipeline: gst::Pipeline,
    src: gst_app::AppSrc,
    codec_data: Vec<u8>,
    first_elapsed: i64,
    clock_rate: u32,
}

#[derive(Default)]
struct Fragments {
    open: Option<String>,
    ends: HashMap<String, i64>,
}

impl SegmentWriter {
    fn start(court: i64, db: Db, dir: &Path, codec_data: Vec<u8>, first_elapsed: i64, clock_rate: u32) -> Result<Self, BoxError> {
        let pipeline = gst::Pipeline::new();
        let src = gst_app::AppSrc::builder()
            .is_live(true)
            .format(gst::Format::Time)
            .caps(
                &gst::Caps::builder("video/x-h264")
                    .field("stream-format", "avc")
                    .field("alignment", "au")
                    .field("codec_data", gst::Buffer::from_slice(codec_data.clone()))
                    .build(),
            )
            .build();
        let parse = gst::ElementFactory::make("h264parse").build()?;
        let sink = gst::ElementFactory::make("splitmuxsink")
            .property("max-size-time", SEGMENT_NS)
            .property("send-keyframe-requests", false)
            .property_from_str("muxer-factory", "mp4mux")
            .build()?;
        pipeline.add_many([src.upcast_ref(), &parse, &sink])?;
        gst::Element::link_many([src.upcast_ref(), &parse, &sink])?;

        // A fragment opens when its first keyframe arrives live, so the box clock at that moment is its start; it
        // ends where the next one starts (or when recording stops). Clock corrections (FR-38) apply immediately.
        let frags: Arc<Mutex<Fragments>> = Arc::default();
        let (dir, frags_open, db_open) = (dir.to_path_buf(), frags.clone(), db.clone());
        sink.connect("format-location-full", false, move |_| {
            let start_ms = clock::now().timestamp_millis();
            let path = dir.join(format!("{start_ms}.mp4")).to_string_lossy().to_string();
            let mut f = frags_open.lock().unwrap_or_else(|p| p.into_inner());
            if let Some(previous) = f.open.replace(path.clone()) {
                f.ends.insert(previous, start_ms);
            }
            let _ = db_open.conn().execute(
                "INSERT OR REPLACE INTO segment (court_id, start_ms, end_ms, path) VALUES (?1, ?2, NULL, ?3)",
                rusqlite::params![court, start_ms, path],
            );
            Some(path.to_value())
        });
        let (bus, frags_bus) = (pipeline.bus().ok_or("pipeline has no bus")?, frags.clone());
        bus.set_sync_handler(move |_, msg| {
            if let gst::MessageView::Element(e) = msg.view()
                && let Some(s) = e.structure()
                && s.name() == "splitmuxsink-fragment-closed"
                && let Ok(location) = s.get::<String>("location")
            {
                let end_ms = frags_bus.lock().unwrap_or_else(|p| p.into_inner()).ends.remove(&location).unwrap_or_else(|| clock::now().timestamp_millis());
                let _ = db.conn().execute("UPDATE segment SET end_ms = ?1 WHERE path = ?2", rusqlite::params![end_ms, location]);
            }
            gst::BusSyncReply::Pass
        });
        pipeline.set_state(gst::State::Playing)?;
        Ok(Self { pipeline, src, codec_data, first_elapsed, clock_rate })
    }

    fn push(&mut self, elapsed: i64, keyframe: bool, data: Vec<u8>) -> Result<(), BoxError> {
        let ticks = (elapsed - self.first_elapsed).max(0) as u128;
        let pts = gst::ClockTime::from_nseconds((ticks * 1_000_000_000 / self.clock_rate as u128) as u64);
        let mut buffer = gst::Buffer::from_mut_slice(data);
        {
            let b = buffer.get_mut().expect("new buffer is writable");
            b.set_pts(pts);
            if !keyframe {
                b.set_flags(gst::BufferFlags::DELTA_UNIT);
            }
        }
        self.src.push_buffer(buffer).map_err(|e| format!("push failed: {e:?}"))?;
        Ok(())
    }

    /// Closes the open fragment properly (writes the MP4 index) before the pipeline stops.
    fn finish(self) {
        let _ = self.src.end_of_stream();
        if let Some(bus) = self.pipeline.bus() {
            let _ = bus.timed_pop_filtered(gst::ClockTime::from_seconds(5), &[gst::MessageType::Eos, gst::MessageType::Error]);
        }
        let _ = self.pipeline.set_state(gst::State::Null);
    }
}

pub struct DiskUse {
    pub used_pct: u64,
    pub free_gb: f64,
}

pub fn disk_use(path: &Path) -> Option<DiskUse> {
    let c = std::ffi::CString::new(path.to_string_lossy().as_bytes()).ok()?;
    let mut s: libc::statvfs = unsafe { std::mem::zeroed() };
    // SAFETY: valid NUL-terminated path and a zeroed out-parameter of the right type.
    if unsafe { libc::statvfs(c.as_ptr(), &mut s) } != 0 {
        return None;
    }
    let (total, free) = (s.f_blocks as u64 * s.f_frsize as u64, s.f_bavail as u64 * s.f_frsize as u64);
    (total > 0).then(|| DiskUse { used_pct: (total - free) * 100 / total, free_gb: free as f64 / 1e9 })
}

pub fn temperature_c() -> Option<f64> {
    let zones = std::fs::read_dir("/sys/class/thermal").ok()?;
    zones
        .filter_map(|z| std::fs::read_to_string(z.ok()?.path().join("temp")).ok()?.trim().parse::<f64>().ok())
        .map(|milli| milli / 1000.0)
        .reduce(f64::max)
}

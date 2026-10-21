//! Small GStreamer jobs shared by the console and the cutter.

use crate::web::AppError;
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;

/// One JPEG still (640×360) from the first frame of a recorded segment.
pub fn still_jpeg(path: &str) -> Result<Vec<u8>, AppError> {
    let fail = |e: &dyn std::fmt::Display| AppError::Internal(format!("preview: {e}"));
    let pipeline = gst::parse::launch(
        "filesrc name=src ! qtdemux ! h264parse ! decodebin ! videoconvert ! videoscale \
         ! video/x-raw,width=640,height=360 ! jpegenc ! appsink name=sink max-buffers=1 sync=false",
    )
    .map_err(|e| fail(&e))?
    .downcast::<gst::Pipeline>()
    .map_err(|_| fail(&"not a pipeline"))?;
    // The path is set as a property, never pasted into the pipeline description.
    pipeline.by_name("src").ok_or_else(|| fail(&"no src"))?.set_property("location", path);
    let sink = pipeline.by_name("sink").and_then(|s| s.downcast::<gst_app::AppSink>().ok()).ok_or_else(|| fail(&"no sink"))?;
    pipeline.set_state(gst::State::Playing).map_err(|e| fail(&e))?;
    let sample = sink.try_pull_sample(gst::ClockTime::from_seconds(5));
    let _ = pipeline.set_state(gst::State::Null);
    let buffer = sample.as_ref().and_then(|s| s.buffer()).ok_or_else(|| fail(&"no frame decoded"))?;
    let map = buffer.map_readable().map_err(|e| fail(&e))?;
    Ok(map.as_slice().to_vec())
}

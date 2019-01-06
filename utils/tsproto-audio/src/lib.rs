use futures::{Future, Stream};
use gstreamer as gst;
use gstreamer::prelude::*;
use gstreamer_app as gst_app;
use gstreamer_audio as gst_audio;
use parking_lot::{Once, ONCE_INIT};
use slog::{debug, error, Logger};

const VOICE_TIMEOUT_SECS: u64 = 1;

pub mod ts_to_audio;
pub mod audio_to_ts;

// TODO Wait until song is finished

/// Has to be called (at least) once before anything else to initialize
/// gstreamer.
pub fn init() {
	static GST_INIT: Once = ONCE_INIT;
	GST_INIT.call_once(|| {
		gst::init().expect("gstreamer failed to initialize");
	});
}

fn main_loop(
	pipeline: &gst::Pipeline,
	logger: Logger,
) -> Result<Box<Future<Item = (), Error = ()> + Send>, failure::Error>
{
	pipeline.set_state(gst::State::Playing).into_result()?;
	debug!(logger, "Pipeline is playing");

	let bus = pipeline
		.get_bus()
		.expect("Pipeline without bus. Shouldn't happen!");

	Ok(Box::new(gst::BusStream::new(&bus).for_each(move |msg| {
		use gstreamer::MessageView;

		let quit = match msg.view() {
			MessageView::Eos(_) => {
				debug!(logger, "Got end of playing stream");
				true
			}
			MessageView::Error(err) => {
				error!(logger,
					"gstreamer pipeline error";
					"src" => ?err.get_src().map(|s| s.get_path_string()),
					"error" => %err.get_error(),
					"debug" => ?err.get_debug()
				);
				true
			}
			MessageView::StateChanged(change) => {
				change.get_current() == gst::State::Null
			}
			_ => false,
		};

		if quit {
			Err(())
		} else {
			Ok(())
		}
	})))
}

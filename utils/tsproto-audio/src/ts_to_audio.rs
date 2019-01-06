//! The main struct is [`Pipeline`].
//!
//! [`Pipeline`]: struct.Pipeline.html

use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

use byteorder::{NetworkEndian, WriteBytesExt};
use failure::format_err;
use gstreamer::gst_element_error;
use num_traits::cast::ToPrimitive;
use parking_lot::Mutex;
use slog::{o, warn, Logger};
use tokio::executor::Executor;
use tokio::timer::Delay;
use tsproto::packets::AudioData;

use super::*;

#[derive(Clone, Debug)]
pub struct AudioPacketHandler {
	appsrc: gst_app::AppSrc,
}

impl AudioPacketHandler {
    pub fn new(appsrc: gst_app::AppSrc) -> Self {
        Self { appsrc }
    }

    pub fn handle_audio_packet(&self, packet: &AudioData) -> Result<(), gst::FlowError> {
		// TODO Whisper packets
		if let AudioData::S2C { id, from, codec, data } = packet {
			let mut buffer =
				gst::Buffer::with_size(data.len() + 5).unwrap();

			{
				let buffer = buffer.get_mut().unwrap();

				//let clock = self.appsrc.get_clock().unwrap();
				//println!("Push buffer {} at {}", id, clock.get_time() - self.appsrc.get_base_time());

				let mut bdata = buffer.map_writable().unwrap();
				let mut bdata = &mut *bdata;
				bdata.write_u16::<NetworkEndian>(*id).unwrap();
				bdata.write_u16::<NetworkEndian>(*from).unwrap();
				bdata.write_u8(codec.to_u8().unwrap()).unwrap();
				bdata.write_all(data).unwrap();
			}

			let res = self.appsrc.push_buffer(buffer);
			if let Err(e) = res.into_result() {
				// Failed to push buffer to source
				return Err(e.into());
			}
		}
		Ok(())
    }
}

pub struct Pipeline {
	pipeline: gst::Pipeline,
	appsrc: gst_app::AppSrc,
}

impl Pipeline {
	/// We need an explicit executor because we want to spawn new tasks in callbacks
	/// from gstreamer threads.
	///
	/// DO NOT PASS A `DefaultExecutor` TO THIS METHOD. The `DefaultExecutor`
	/// only resolves to an actual executor when `spawn` is called.
	pub fn new<E: Executor + Clone + Send + Sync + 'static>(logger: Logger, executor: E)
		-> Result<Self, failure::Error> {
		init();
		let logger = logger.new(o!("pipeline" => "ts-to-audio"));
		let pipeline = gst::Pipeline::new("ts-to-audio-pipeline");

		let appsrc = gst::ElementFactory::make("appsrc", "appsrc")
			.ok_or_else(|| format_err!("Missing appsrc"))?;
		let demuxer = gst::ElementFactory::make("ts3audiodemux", "demuxer")
			.ok_or_else(|| format_err!("Missing ts3audiodemux"))?;

		let mixer = gst::ElementFactory::make("audiomixer", "mixer")
			.ok_or_else(|| format_err!("Missing audiomixer"))?;
		let queue = gst::ElementFactory::make("queue", "queue")
			.ok_or_else(|| format_err!("Missing queue"))?;

		// The latency with autoaudiosink is high
		// Linux: Try pulsesink, alsasink
		// Windows: Try directsoundsink
		// Else use autoaudiosink
		let mut autosink = None;
		#[cfg(target_os = "linux")]
		{
			if autosink.is_none() {
				if let Some(sink) =
					gst::ElementFactory::make("pulsesink", "autosink")
				{
					autosink = Some(sink);
				}
			}
		}
		#[cfg(target_os = "linux")]
		{
			if autosink.is_none() {
				if let Some(sink) =
					gst::ElementFactory::make("alsasink", "autosink")
				{
					autosink = Some(sink);
				}
			}
		}

		#[cfg(target_os = "windows")]
		{
			if autosink.is_none() {
				if let Some(sink) =
					gst::ElementFactory::make("directsoundsink", "autosink")
				{
					autosink = Some(sink);
				}
			}
		}

		let autosink = if let Some(sink) = autosink {
			sink
		} else {
			gst::ElementFactory::make("pulsesink", "autosink")
				.ok_or_else(|| format_err!("Missing autoaudiosink"))?
		};
		if autosink.has_property("buffer-time", None).is_ok() {
			autosink
				.set_property("buffer-time", &::glib::Value::from(&20_000i64))?;
		}
		if autosink.has_property("blocksize", None).is_ok() {
			autosink.set_property("blocksize", &::glib::Value::from(&960u32))?;
		}

		let src = appsrc.clone().dynamic_cast::<gst_app::AppSrc>().unwrap();
		src.set_caps(&gst::Caps::new_simple("audio/x-ts3audio", &[]));
		src.set_property_format(gst::Format::Time);
		src.set_property_min_latency((gst::SECOND_VAL / 50) as i64); // 20 ms in ns
		src.set_property_min_latency(0); // in ns
								  	  // Important to reduce the playback latency
		src.set_property("do-timestamp", &::glib::Value::from(&true))?;
		// Set as live source, which means it does not produce data when paused
		src.set_property("is-live", &::glib::Value::from(&true))?;

		// The audiotestsrc just has to exist and not be eos.
		// Without the audiotestsrc, the audiomixer would send eos after the last
		// pad is removed and the pipeline would finish.
		let fakesrc = gst::ElementFactory::make("audiotestsrc", "fake")
			.ok_or_else(|| format_err!("Missing audiotestsrc"))?;
		fakesrc.set_property("do-timestamp", &::glib::Value::from(&true))?;
		fakesrc.set_property("is-live", &::glib::Value::from(&true))?;
		fakesrc.set_property("samplesperbuffer", &::glib::Value::from(&960i32))?; // 20ms at 48 000 kHz
		fakesrc.set_property_from_str("wave", "Silence");

		pipeline
			.add_many(&[&appsrc, &demuxer, &fakesrc, &mixer, &queue, &autosink])?;
		gst::Element::link_many(&[&appsrc, &demuxer])?;
		gst::Element::link_many(&[&mixer, &queue, &autosink])?;
		// Additionally, we use the audiotestsrc to force the output to stereo and 48000 kHz
		fakesrc.link_filtered(
			&mixer,
			&gst::Caps::new_simple(
				"audio/x-raw",
				&[("rate", &48000i32), ("channels", &2i32)],
			),
		)?;

		// Set playing only when someone sends audio
		pipeline
			.set_state(gst::State::Playing)
			.into_result()
			.unwrap();
		mixer.set_state(gst::State::Paused).into_result().unwrap();
		queue.set_state(gst::State::Paused).into_result().unwrap();
		autosink
			.set_state(gst::State::Paused)
			.into_result()
			.unwrap();
		fakesrc.set_state(gst::State::Paused).into_result().unwrap();

		let pipe = pipeline.clone();
		let logger2 = logger.clone();
		// Link demuxer to mixer when a new client speaks
		demuxer.connect_pad_added(move |demuxer, src_pad| {
			debug!(logger, "Got new client pad"; "name" => src_pad.get_name());
			// Create decoder
			// TODO Create fitting decoder and maybe audioresample to 48 kHz
			let decode = gst::ElementFactory::make(
				"decodebin",
				format!("decoder_{}", src_pad.get_name()).as_str(),
			)
			.expect("Missing decodebin");
			if let Err(e) = pipe.add(&decode) {
				error!(logger, "Cannot add decoder to pipeline"; "error" => ?e);
				return;
			}

			// Link to sink pad decoder
			let sink_pad = decode
				.get_static_pad("sink")
				.expect("Next element has no sink pad");
			if let Err(error) = src_pad.link(&sink_pad).into_result() {
				error!(logger, "Cannot link pads"; "error" => ?error);
				gst_element_error!(
					demuxer,
					gst::ResourceError::Failed,
					("Failed to link decoder")
				);
			}

			decode.sync_state_with_parent().unwrap();
			let logger = logger.clone();
			let mixer = mixer.clone();
			let decoder = decode.clone();
			let demuxer = demuxer.clone();
			let pipe = pipe.clone();
			let sink = autosink.clone();
			let queue = queue.clone();
			let executor = executor.clone();
			decode.connect_pad_added(move |dbin, src_pad| {
				debug!(logger, "Got new client decoder pad"; "name" => src_pad.get_name());

				// Link to sink pad of next element
				let first_pad = mixer.iterate_sink_pads().skip(1).next().is_none();
				let sink_pad = mixer
					.get_request_pad("sink_%u")
					.expect("Next element has no sink pad");
				if let Err(error) = src_pad.link(&sink_pad).into_result() {
					error!(logger, "Cannot link pads"; "error" => ?error);
					gst_element_error!(
						dbin,
						gst::ResourceError::Failed,
						("Failed to link decoder")
					);
				}

				let decode = decoder.clone();
				let logger = logger.clone();
				let mix = mixer.clone();
				let demuxer = demuxer.clone();
				let pipeline = pipe.clone();
				let autosink = sink.clone();
				let queue2 = queue.clone();
				let src_pad2 = src_pad.clone();
				let last_sent = Arc::new(Mutex::new(Instant::now()));
				let last = last_sent.clone();
				// Set as active if a buffer was sent
				src_pad.add_probe(
					gst::PadProbeType::DATA_DOWNSTREAM,
					move |_pad, _info| {
						let mut last_sent = last.lock();
						*last_sent = Instant::now();
						gst::PadProbeReturn::Ok
					},
				);

				// Check every second if the stream timed out
				let logger2 = logger.clone();
				let func = move || {
					// Get latency
					// Mixer has 30 ms
					// autosink 220 ms
					let mut query = gst::Query::new_latency();
					if pipeline.get_by_name("autosink").unwrap().query(&mut query) {
						match query.view() {
							gst::QueryView::Latency(l) => {
								println!("Latency: {:?}", l.get_result())
							}
							_ => {}
						}
					}

					let decode = decode.clone();
					let logger = logger.clone();
					let mix = mix.clone();
					let demuxer = demuxer.clone();
					let pipeline = pipeline.clone();
					let autosink = autosink.clone();
					let queue2 = queue2.clone();
					let src_pad2 = src_pad2.clone();

					let last_pad = mix.iterate_sink_pads().skip(2).next().is_none();
					if last_pad {
						// Pause pipeline
						mix.set_state(gst::State::Paused).into_result().unwrap();
						queue2.set_state(gst::State::Paused).into_result().unwrap();
						autosink
							.set_state(gst::State::Paused)
							.into_result()
							.unwrap();
					}

					// Unlink and remove decoder
					debug!(logger, "Remove client decoder");

					let mixer_pad = src_pad2.get_peer();
					let decode_sink_pad = decode.iterate_sink_pads().next();
					let demuxer_pad = decode_sink_pad
						.and_then(|p| p.ok())
						.and_then(|p| p.get_peer());

					gst::Element::unlink_many(&[&demuxer, &decode, &mix]);

					// Remove pad from mixer
					if let Some(pad) = mixer_pad {
						if let Err(e) = mix.remove_pad(&pad) {
							error!(logger, "Cannot remove mixer pad"; "error" => ?e);
						}
					} else {
						error!(logger, "Cannot find mixer pad";);
					}

					// Remove pad from demuxer
					if let Some(pad) = demuxer_pad {
						if let Err(e) = demuxer.remove_pad(&pad) {
							error!(logger, "Cannot remove demuxer pad"; "error" => ?e);
						}
					} else {
						error!(logger, "Cannot find demuxer pad";);
					}

					if let Err(e) = pipeline.remove(&decode) {
						error!(logger, "Cannot remove decoder from pipeline"; "error" => ?e);
					}
					// Cleanup
					decode.set_state(gst::State::Null).into_result().unwrap();
				};

				voice_timeout(executor.clone(), logger2, func, last_sent);

				if first_pad {
					// Start pipeline
					sink.set_state(gst::State::Playing).into_result().unwrap();
					queue.set_state(gst::State::Playing).into_result().unwrap();
					mixer.set_state(gst::State::Playing).into_result().unwrap();
				}
			});
		});

		// Run event handler in background
		tokio::spawn(main_loop(&pipeline, logger2)?);

		Ok(Self {
			pipeline,
			appsrc: src,
		})
	}

	pub fn get_pipeline(&self) -> &gst::Pipeline { &self.pipeline }

	pub fn create_packet_handler(&self) -> AudioPacketHandler {
		AudioPacketHandler::new(self.appsrc.clone())
	}
}

impl Drop for Pipeline {
	fn drop(&mut self) {
		// Cleanup gstreamer
		// TODO Sometimes segfaults here when gstreamer posts onto bus
		// Is the bus still running or so?
		self.pipeline.set_state(gst::State::Null).into_result().expect(
			"Failed to shutdown gstreamer pipeline");
	}
}

// We need an explicit executor because we want to spawn new tasks in callbacks
// from gstreamer threads.
fn voice_timeout<E: Executor + Clone + Send + Sync + 'static, F: FnOnce() + Send + 'static>(
	mut executor: E,
	logger: Logger,
	f: F,
	last_sent: Arc<Mutex<Instant>>,
)
{
	let timeout =
		Delay::new(Instant::now() + Duration::from_secs(VOICE_TIMEOUT_SECS));
	let e = executor.clone();
	let l = logger.clone();
	if let Err(e) = executor.spawn(Box::new(timeout.then(move |_| {
		let last = *last_sent.lock();
		if Instant::now().duration_since(last).as_secs() >= VOICE_TIMEOUT_SECS {
			f();
		} else {
			voice_timeout(e, l, f, last_sent);
		}
		Ok(())
	}))) {
		warn!(logger, "Failed to spawn voice timeout"; "error" => ?e);
	}
}

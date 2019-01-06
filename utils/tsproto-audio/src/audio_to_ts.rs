//! The main struct is [`Pipeline`].
//!
//! [`Pipeline`]: struct.Pipeline.html

use std::fmt::Debug;

use failure::format_err;
use futures::Sink;
use gstreamer::{gst_element_error, gst_element_warning};
use gstreamer_audio::StreamVolumeExt;
use slog::{o, Logger};
use tokio::executor::Executor;
use tsproto::packets::{AudioData, CodecType, OutAudio, OutPacket};

use super::*;

pub trait PacketSinkCreator<E>: Send + Sync + 'static {
	type S: Sink<SinkItem=OutPacket, SinkError=E> + Send + 'static;
	fn get_sink(&self) -> Self::S;
}

pub struct Pipeline {
	logger: Logger,
	pipeline: gst::Pipeline,
	volume: gst_audio::StreamVolume,
}

impl Pipeline {
	/// We need an explicit executor because we want to spawn new tasks in callbacks
	/// from gstreamer threads.
	///
	/// DO NOT PASS A `DefaultExecutor` TO THIS METHOD. The `DefaultExecutor`
	/// only resolves to an actual executor when `spawn` is called.
	pub fn new<Er: Debug, PS: PacketSinkCreator<Er>, E: Executor + Clone + Send + Sync + 'static>(
		logger: Logger,
		packet_sink_creator: PS,
		executor: E,
		uri: Option<&str>,
	) -> Result<Self, failure::Error> {
		init();
		let logger = logger.new(o!("pipeline" => "audio-to-ts"));
		let pipeline = gst::Pipeline::new("audio-to-ts-pipeline");

		let decode;
		if let Some(uri) = &uri {
			decode = gst::ElementFactory::make("uridecodebin", "decode")
				.ok_or_else(|| format_err!("Missing uridecodebin"))?;
			decode.set_property("uri", &glib::Value::from(uri))?;
		} else {
			decode = gst::ElementFactory::make("autoaudiosrc", "audiosrc")
				.ok_or_else(|| format_err!("Missing autoaudiosrc"))?;
		}

		let resampler = gst::ElementFactory::make("audioresample", "resample")
			.ok_or_else(|| format_err!("Missing audioresample"))?;

		let vol = gst::ElementFactory::make("volume", "vol")
			.ok_or_else(|| format_err!("Missing volume"))?;

		let opusenc = gst::ElementFactory::make("opusenc", "opusenc")
			.ok_or_else(|| format_err!("Missing opusenc"))?;
		let sink = gst::ElementFactory::make("appsink", "appsink")
			.ok_or_else(|| format_err!("Missing appsink"))?;

		opusenc.set_property_from_str("bitrate-type", "vbr");
		opusenc.set_property_from_str("audio-type", "voice"); // or generic
		// Discontinuous transmission: Reduce bandwidth of silence
		// Unfortunately creates artifacts
		//opusenc.set_property("dtx", &glib::Value::from(&true))?;
		// Inband forward error correction
		opusenc.set_property("inband-fec", &glib::Value::from(&true))?;
		// Packetloss between 0 - 100
		opusenc.set_property("packet-loss-percentage", &glib::Value::from(&0))?;

		pipeline.add_many(&[&decode, &resampler, &vol, &opusenc, &sink])?;
		gst::Element::link_many(&[&resampler, &vol, &opusenc, &sink])?;
		if uri.is_none() {
			decode.link(&resampler)?;
		}

		// Link decode to next element if a pad gets available
		let next = resampler;
		let logger2 = logger.clone();
		decode.connect_pad_added(move |dbin, src_pad| {
			debug!(logger2, "Got new pad"; "name" => src_pad.get_name());
			let is_audio = src_pad.get_current_caps().and_then(|caps| {
				caps.get_structure(0).map(|s| {
					debug!(logger2, "Capabilities"; "name" => src_pad.get_name(),
						"caps" => ?s);
					s.get_name().starts_with("audio/")
				})
			});

			let is_audio = if let Some(is_audio) = is_audio {
				is_audio
			} else {
				gst_element_warning!(
					dbin,
					gst::CoreError::Negotiation,
					("Failed to get media type from pad {}", src_pad.get_name())
				);
				return;
			};
			if !is_audio {
				return;
			}

			// Link to sink pad of next element
			let sink_pad = next
				.get_static_pad("sink")
				.expect("Next element has no sink pad");
			if let Err(error) = src_pad.link(&sink_pad).into_result() {
				error!(logger2, "Cannot link pads"; "error" => ?error);
				gst_element_error!(
					dbin,
					gst::ResourceError::Failed,
					("Failed to link decoder")
				);
			}
		});

		let streamvolume = vol.dynamic_cast::<gst_audio::StreamVolume>().unwrap();

		let appsink = sink.dynamic_cast::<gst_app::AppSink>().unwrap();

		let logger2 = logger.clone();
		appsink.set_callbacks(
			gst_app::AppSinkCallbacks::new()
				.new_sample(move |appsink| {
					let sample = match appsink.pull_sample() {
						None => return gst::FlowReturn::Eos,
						Some(sample) => sample,
					};

					let buffer = if let Some(buffer) = sample.get_buffer() {
						buffer
					} else {
						gst_element_error!(
							appsink,
							gst::ResourceError::Failed,
							("Failed to get buffer from appsink")
						);

						return gst::FlowReturn::Error;
					};

					let map = if let Some(map) = buffer.map_readable() {
						map
					} else {
						gst_element_error!(
							appsink,
							gst::ResourceError::Failed,
							("Failed to map buffer readable")
						);

						return gst::FlowReturn::Error;
					};

					// Create packet
					let packet = OutAudio::new(&AudioData::C2S {
						id: 0,
						codec: CodecType::OpusMusic,
						data: map.as_slice(),
					});

					// Write into packet sink
					let logger2 = logger.clone();
					if let Err(e) = executor.clone().spawn(Box::new(packet_sink_creator.get_sink().send(packet)
						.map(|_| ())
						.map_err(move |e| {
							error!(logger2, "Failed to send packet"; "error" => ?e);
						}))) {
						error!(logger, "Failed to start packet sending"; "error" => ?e);
					}
					gst::FlowReturn::Ok
				})
				.build(),
		);

		// Run event handler in background
		tokio::spawn(main_loop(&pipeline, logger2.clone())?);

		Ok(Self {
			logger: logger2,
			pipeline,
			volume: streamvolume,
		})
	}

	pub fn set_volume(&self, volume: f64) {
		self.volume.set_volume(gst_audio::StreamVolumeFormat::Linear, volume);
	}

	pub fn is_playing(&self) -> Result<bool, failure::Error> {
		// Returns (success, current state, pending state)
		let state = self.pipeline.get_state(
			gst::ClockTime::from_mseconds(10),
		);

		if state.0 != gst::StateChangeReturn::Failure {
			if state.1 == gst::State::Playing {
				Ok(true)
			} else if state.1 == gst::State::Paused {
				Ok(false)
			} else {
				Err(format_err!("State is neither playing nor paused ({:?})", state.1))
			}
		} else {
			Err(format_err!("Failed to get current state ({:?})", state))
		}
	}

	pub fn set_playing(&self, playing: bool) -> Result<(), failure::Error> {
		if playing {
			debug!(self.logger, "Change to playing");
			self.pipeline.set_state(gst::State::Playing).into_result()?;
		} else {
			debug!(self.logger, "Change to paused");
			self.pipeline.set_state(gst::State::Paused).into_result()?;
		}
		Ok(())
	}
}

impl Drop for Pipeline {
	fn drop(&mut self) {
		// Cleanup gstreamer
		self.pipeline.set_state(gst::State::Null).into_result().expect(
			"Failed to shutdown gstreamer pipeline");
	}
}

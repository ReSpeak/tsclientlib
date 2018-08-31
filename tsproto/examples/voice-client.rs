extern crate base64;
#[macro_use]
extern crate failure;
extern crate futures;
extern crate glib;
#[macro_use]
extern crate gstreamer as gst;
extern crate gstreamer_app as gst_app;
extern crate gstreamer_audio as gst_audio;
extern crate num_traits;
extern crate ring;
#[macro_use]
extern crate slog;
extern crate slog_async;
extern crate slog_perf;
extern crate slog_term;
extern crate structopt;
extern crate tokio_core;
extern crate tokio_signal;
extern crate tsproto;

use std::cell::RefCell;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::{future, Future, Sink, Stream};
use futures::sync::mpsc;
use gst::prelude::*;
use gst_audio::StreamVolumeExt;
use num_traits::cast::ToPrimitive;
use slog::{Drain, Logger};
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::{Core, Handle, Remote, Timeout};
#[cfg(target_family = "unix")]
use tokio_signal::unix::{Signal, SIGHUP};
use tsproto::*;
use tsproto::client::ClientData;
use tsproto::packets::*;

mod utils;
use utils::*;
use utils::voice::IncommingVoiceHandler;

const VOICE_TIMEOUT_SECS: u64 = 1;

#[derive(StructOpt, Debug)]
#[structopt(raw(global_settings =
    "&[AppSettings::ColoredHelp, AppSettings::VersionlessSubcommands]"))]
struct Args {
    #[structopt(short = "a", long = "address",
                default_value = "127.0.0.1:9987",
                help = "The address of the server to connect to")]
    address: SocketAddr,
    #[structopt(long = "local-address", default_value = "0.0.0.0:0",
                help = "The listening address of the client")]
    local_address: SocketAddr,
    #[structopt(long = "uri", short = "u", default_value = "",
                help = "The URI of the audio, which should be played. \
                    If it is empty, it will capture from a microphone.")]
    uri: String,
    #[structopt(long = "volume", short = "v", default_value = "1.0",
                help = "The volume of audio-to-ts.")]
    volume: f64,
    #[structopt(long = "no-input", help = "Disable audio-to-ts.")]
    no_input: bool,
    #[structopt(long = "no-output", help = "Disable ts-to-audio.")]
    no_output: bool,
}

fn main() {
    tsproto::init().unwrap();
    gst::init().unwrap();

    // Parse command line options
    let args = Args::from_args();
    let mut core = Core::new().unwrap();
    let handle = core.handle();

    let logger = {
        let decorator = slog_term::TermDecorator::new().build();
        // Or FullFormat
        let drain = slog_term::CompactFormat::new(decorator).build().fuse();
        let drain = slog_async::Async::new(drain).build().fuse();

        Logger::root(drain, o!())
    };

    let c = create_client(args.local_address, core.handle(), logger.clone(), false);

    if !args.no_output {
        // Setup incoming voice handler
        let in_pipe = create_ts_to_audio_pipeline(core.remote(), logger.clone()).unwrap();
        let audio = main_loop(&in_pipe, logger.clone()).unwrap();
        let in_handler = IncommingVoiceHandler::new(logger.clone(), in_pipe).unwrap();
        ClientData::apply_packet_stream_wrapper::<IncommingVoiceHandler>(&c, in_handler);

        // Run event handler in background
        handle.spawn(audio);
    }

    // Connect
    if let Err(error) = core.run(connect(logger.clone(), &handle, c.clone(),
        args.address)) {
        error!(logger, "Failed to connect"; "error" => ?error);
        return;
    }
    info!(logger, "Connected");

    // Disconnect if audio fails
    let pipeline;
    let audio;
    if args.no_input {
        pipeline = None;
        audio = None;
    } else {
        let (pipeline2, audio2) = match setup_audio(&args, Rc::downgrade(&c), handle.clone(), logger.clone()) {
            Err(error) => {
                error!(logger, "Failed to setup audio"; "error" => ?error);

                // Disconnect
                if let Err(error) = core.run(disconnect(c.clone(), args.address)) {
                    error!(logger, "Failed to disconnect"; "error" => ?error);
                }
                return;
            }
            Ok(r) => r,
        };
        pipeline = Some(pipeline2);
        audio = Some(audio2);
    }

    // Wait until song is finished
    //if let Err(_error) = core.run(audio) {
        // Also returns with an error when the stream finished
        //error!(logger, "Error while playing"; "error" => ?error);
    //}
    //info!(logger, "Waited");

    if let Some(audio) = audio {
        handle.spawn(audio);
    }

    // Pause or unpause sending on sighup
    if let Some(pipeline) = &pipeline {
        let pipe = pipeline.clone();
        let logger2 = logger.clone();
        let logger3 = logger.clone();
        #[cfg(target_family = "unix")]
        let sighup = Signal::new(SIGHUP).flatten_stream().for_each(move |_| {
            // Switch state from playing to paused or reverse
            // Returns (success, current state, pending state)
            let state = pipe.get_state(gst::ClockTime::from_mseconds(10));
            if state.0 != gst::StateChangeReturn::Failure {
                //debug!(logger2, "Got state"; "current" => ?state.1, "pending" => ?state.2);
                if state.1 == gst::State::Playing {
                    debug!(logger2, "Change to paused");
                    if let Err(error) = pipe.set_state(gst::State::Paused).into_result() {
                        error!(logger2, "Failed to pause pipeline"; "error" => ?error);
                    }
                } else if state.1 == gst::State::Paused {
                    debug!(logger2, "Change to playing");
                    if let Err(error) = pipe.set_state(gst::State::Playing).into_result() {
                        error!(logger2, "Failed to start pipeline"; "error" => ?error);
                    }
                }
            } else {
                error!(logger2, "Failed to get current state"; "result" => ?state);
            }
            future::ok(())
        }).map_err(move |error| {
            error!(logger3, "Error waiting for signal"; "error" => ?error);
        });
        handle.spawn(sighup);
    }

    // Stop with ctrl + c
    let ctrl_c = tokio_signal::ctrl_c().flatten_stream();
    core.run(ctrl_c.into_future().map(move |_| ()).map_err(move |_| ())).unwrap();

    // Disconnect
    if let Err(error) = core.run(disconnect(c.clone(), args.address)) {
        error!(logger, "Failed to disconnect"; "error" => ?error);
        return;
    }
    info!(logger, "Disconnected");

    // Cleanup gstreamer
    if let Some(pipeline) = pipeline {
        pipeline.set_state(gst::State::Null).into_result().unwrap();
    }
}

// Output audio from other clients: Dynamically add appsources when a new client
// sends audio data.
// Appsource is called Clientsrc-serverid-clientid-codec
// If a client stops (signalled by the appsource), wait a second and then remove
// the appsource again.

fn setup_audio(args: &Args, c: Weak<RefCell<ClientData>>, handle: Handle,
    logger: Logger) -> Result<(gst::Pipeline, Box<Future<Item = (), Error = ()>>), Error> {
    // Channel, which can buffer some packets
    let (send, recv) = mpsc::channel(5);
    let pipeline = create_audio_to_ts_pipeline(send, args, logger.clone())?;
    let audio = main_loop(&pipeline, logger)?;
    handle.spawn(packet_sender(c, recv));
    Ok((pipeline, audio))
}

fn packet_sender(data: Weak<RefCell<ClientData>>,
    recv: mpsc::Receiver<(SocketAddr, Packet)>)
    -> Box<Future<Item = (), Error = ()>> {
    let sink = ClientData::get_packets(data);
    Box::new(recv.forward(sink.sink_map_err(|error| {
        println!("Error when forwarding: {:?}", error);
    })).map(|_| ()).map_err(|error| {
        println!("Error when forwarding: {:?}", error);
    }))
}

fn voice_timeout<F: FnOnce() + 'static>(f: F, last_sent: Arc<Mutex<Instant>>, handle: Handle) {
    let timeout = Timeout::new(Duration::from_secs(VOICE_TIMEOUT_SECS), &handle).unwrap();
    let h = handle.clone();
    handle.spawn(timeout.then(move |_| {
        let last = *last_sent.lock().unwrap();
        if Instant::now().duration_since(last).as_secs() >= VOICE_TIMEOUT_SECS {
            f();
        } else {
            voice_timeout(f, last_sent, h);
        }
        Ok(())
    }));
}

fn create_ts_to_audio_pipeline(handle: Remote, logger: Logger) -> Result<gst::Pipeline, failure::Error> {
    let pipeline = gst::Pipeline::new("ts-to-audio-pipeline");

    let appsrc = gst::ElementFactory::make("appsrc", "appsrc").ok_or_else(||
        format_err!("Missing appsrc"))?;
    let demuxer = gst::ElementFactory::make("ts3audiodemux", "demuxer").ok_or_else(||
        format_err!("Missing ts3audiodemux"))?;

    let mixer = gst::ElementFactory::make("audiomixer", "mixer").ok_or_else(||
        format_err!("Missing audiomixer"))?;
    let queue = gst::ElementFactory::make("queue", "queue").ok_or_else(||
        format_err!("Missing queue"))?;

    // The latency with autoaudiosink is high
    // Linux: Try pulsesink, alsasink
    // Windows: Try directsoundsink
    // Else use autoaudiosink
    let mut autosink = None;
    #[cfg(target_os = "linux")] {
    if autosink.is_none() {
        if let Some(sink) = gst::ElementFactory::make("pulsesink", "autosink") {
            autosink = Some(sink);
        }
    }
    }
    #[cfg(target_os = "linux")] {
    if autosink.is_none() {
        if let Some(sink) = gst::ElementFactory::make("alsasink", "autosink") {
            autosink = Some(sink);
        }
    }
    }

    #[cfg(target_os = "windows")] {
    if autosink.is_none() {
        if let Some(sink) = gst::ElementFactory::make("directsoundsink", "autosink") {
            autosink = Some(sink);
        }
    }
    }

    let autosink = if let Some(sink) = autosink {
        sink
    } else {
        gst::ElementFactory::make("pulsesink", "autosink").ok_or_else(||
            format_err!("Missing autoaudiosink"))?
    };
    if autosink.has_property("buffer-time", None).is_ok() {
        autosink.set_property("buffer-time", &::glib::Value::from(&20_000i64))?;
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
    let fakesrc = gst::ElementFactory::make("audiotestsrc", "fake").ok_or_else(||
        format_err!("Missing audiotestsrc"))?;
    fakesrc.set_property("do-timestamp", &::glib::Value::from(&true))?;
    fakesrc.set_property("is-live", &::glib::Value::from(&true))?;
    fakesrc.set_property("samplesperbuffer", &::glib::Value::from(&960i32))?; // 20ms at 48 000 kHz
    fakesrc.set_property_from_str("wave", "Silence");

    pipeline.add_many(&[&appsrc, &demuxer, &fakesrc, &mixer, &queue, &autosink])?;
    gst::Element::link_many(&[&appsrc, &demuxer])?;
    gst::Element::link_many(&[&mixer, &queue, &autosink])?;
    // Additionally, we use the audiotestsrc to force the output to stereo and 48000 kHz
    fakesrc.link_filtered(&mixer, &gst::Caps::new_simple(
        "audio/x-raw", &[("rate", &48000i32), ("channels", &2i32)]))?;

    // Set playing only when someone sends audio
    pipeline.set_state(gst::State::Playing).into_result().unwrap();
    mixer.set_state(gst::State::Paused).into_result().unwrap();
    queue.set_state(gst::State::Paused).into_result().unwrap();
    autosink.set_state(gst::State::Paused).into_result().unwrap();
    fakesrc.set_state(gst::State::Paused).into_result().unwrap();

    let pipe = pipeline.clone();
    // Link demuxer to mixer when a new client speaks
    demuxer.connect_pad_added(move |demuxer, src_pad| {
        debug!(logger, "Got new client pad"; "name" => src_pad.get_name());
        // Create decoder
        // TODO Create fitting decoder and maybe audioresample to 48 kHz
        let decode = gst::ElementFactory::make("decodebin",
            format!("decoder_{}", src_pad.get_name()).as_str())
            .expect("Missing decodebin");
        if let Err(e) = pipe.add(&decode) {
            error!(logger, "Cannot add decoder to pipeline"; "error" => ?e);
            return;
        }

        // Link to sink pad decoder
        let sink_pad = decode.get_static_pad("sink")
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
        let handle = handle.clone();
        decode.connect_pad_added(move |dbin, src_pad| {
            debug!(logger, "Got new client decoder pad"; "name" => src_pad.get_name());

            // Link to sink pad of next element
            let first_pad = mixer.iterate_sink_pads().skip(1).next().is_none();
            let sink_pad = mixer.get_request_pad("sink_%u")
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
            src_pad.add_probe(gst::PadProbeType::DATA_DOWNSTREAM, move |_pad, info| {
                let mut last_sent = last.lock().unwrap();
                *last_sent = Instant::now();
                gst::PadProbeReturn::Ok
            });

            // Check every second if the stream timed out
            let func = move || {
                // Get latency
                // Mixer has 30 ms
                // autosink 220 ms
                let mut query = gst::Query::new_latency();
                if pipeline.get_by_name("autosink").unwrap().query(&mut query) {
                    match query.view() {
                        gst::QueryView::Latency(l) => println!("Latency: {:?}", l.get_result()),
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
                    autosink.set_state(gst::State::Paused).into_result().unwrap();
                }

                // Unlink and remove decoder
                debug!(logger, "Remove client decoder");

                let mixer_pad = src_pad2.get_peer();
                let decode_sink_pad = decode.iterate_sink_pads().next();
                let demuxer_pad = decode_sink_pad.and_then(|p| p.ok())
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

            handle.spawn(move |h| {
                voice_timeout(func, last_sent, h.clone());
                Ok(())
            });

            if first_pad {
                // Start pipeline
                sink.set_state(gst::State::Playing).into_result().unwrap();
                queue.set_state(gst::State::Playing).into_result().unwrap();
                mixer.set_state(gst::State::Playing).into_result().unwrap();
            }
        });
    });

    Ok(pipeline)
}

fn create_audio_to_ts_pipeline(sender: mpsc::Sender<(SocketAddr, Packet)>, args: &Args,
    logger: Logger) -> Result<gst::Pipeline, failure::Error> {
    let pipeline = gst::Pipeline::new("audio-to-ts-pipeline");

    let decode;
    if args.uri.is_empty() {
        decode = gst::ElementFactory::make("autoaudiosrc", "audiosrc").ok_or_else(||
            format_err!("Missing autoaudiosrc"))?;
    } else {
        decode = gst::ElementFactory::make("uridecodebin", "decode").ok_or_else(||
            format_err!("Missing uridecodebin"))?;
        decode.set_property("uri", &glib::Value::from(&args.uri))?;
    }

    let resampler = gst::ElementFactory::make("audioresample", "resample").ok_or_else(||
        format_err!("Missing audioresample"))?;

    let volume = gst::ElementFactory::make("volume", "volume").ok_or_else(||
        format_err!("Missing volume"))?;

    let opusenc = gst::ElementFactory::make("opusenc", "opusenc").ok_or_else(||
        format_err!("Missing opusenc"))?;
    let sink = gst::ElementFactory::make("appsink", "appsink").ok_or_else(||
        format_err!("Missing appsink"))?;

    opusenc.set_property_from_str("bitrate-type", "vbr");
    opusenc.set_property_from_str("audio-type", "voice"); // or generic
    // Discontinuous transmission: Reduce bandwidth of silence
    // Unfortunately creates artifacts
    //opusenc.set_property("dtx", &glib::Value::from(&true))?;
    // Inband forward error correction
    opusenc.set_property("inband-fec", &glib::Value::from(&true))?;
    // Packetloss between 0 - 100
    opusenc.set_property("packet-loss-percentage", &glib::Value::from(&0))?;

    pipeline.add_many(&[&decode, &resampler, &volume, &opusenc, &sink])?;
    gst::Element::link_many(&[&resampler, &volume, &opusenc, &sink])?;
    if args.uri.is_empty() {
        decode.link(&resampler)?;
    }

    // Link decode to next element if a pad gets available
    let next = resampler;
    let addr = args.address;
    decode.connect_pad_added(move |dbin, src_pad| {
        debug!(logger, "Got new pad"; "name" => src_pad.get_name());
        let is_audio = src_pad.get_current_caps().and_then(|caps| {
            caps.get_structure(0).map(|s| {
                debug!(logger, "Capabilities"; "name" => src_pad.get_name(),
                    "caps" => ?s);
                s.get_name().starts_with("audio/")
            })
        });

        let is_audio = if let Some(is_audio) = is_audio {
            is_audio
        } else {
            gst_element_warning!(dbin, gst::CoreError::Negotiation,
                ("Failed to get media type from pad {}", src_pad.get_name()));
            return;
        };
        if !is_audio {
            return;
        }

        // Link to sink pad of next element
        let sink_pad = next.get_static_pad("sink")
            .expect("Next element has no sink pad");
        if let Err(error) = src_pad.link(&sink_pad).into_result() {
            error!(logger, "Cannot link pads"; "error" => ?error);
            gst_element_error!(
                dbin,
                gst::ResourceError::Failed,
                ("Failed to link decoder")
            );
        }
    });


    let streamvolume = volume.dynamic_cast::<gst_audio::StreamVolume>().unwrap();
    streamvolume.set_volume(gst_audio::StreamVolumeFormat::Linear, args.volume);

    let appsink = sink.dynamic_cast::<gst_app::AppSink>().unwrap();

    let sender = Mutex::new(sender);
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
                let header = packets::Header::new(packets::PacketType::Voice);
                let data = packets::Data::VoiceC2S {
                    id: 0,
                    codec_type: packets::CodecType::OpusMusic.to_u8().unwrap(),
                    voice_data: map.as_slice().to_vec(),
                };
                let packet = packets::Packet::new(header, data);

                // Write into packet sink
                match sender.lock().unwrap().try_send((addr, packet)) {
                    Ok(()) => gst::FlowReturn::Ok,
                    Err(error) => {
                        gst_element_error!(
                            appsink,
                            gst::ResourceError::Failed,
                            ("Failed to send packet")
                        );
                        println!("Failed to send packet: {:?}", error);

                        return gst::FlowReturn::Error;
                    }
                }
            })
            .build(),
    );

    Ok(pipeline)
}

fn main_loop(pipeline: &gst::Pipeline, logger: Logger)
    -> Result<Box<Future<Item=(), Error = ()>>, failure::Error> {
    pipeline.set_state(gst::State::Playing).into_result()?;
    debug!(logger, "Pipeline is playing");

    let bus = pipeline
        .get_bus()
        .expect("Pipeline without bus. Shouldn't happen!");

    Ok(Box::new(gst::BusStream::new(&bus).for_each(move |msg| {
        use gst::MessageView;

        let quit = match msg.view() {
            MessageView::Eos(..) => {
                debug!(logger, "Got end of playing stream");
                true
            }
            MessageView::Error(err) => {
                error!(logger,
                    "gstreamer audio-to-ts pipeline error";
                    "src" => ?err.get_src().map(|s| s.get_path_string()),
                    "error" => %err.get_error(),
                    "debug" => ?err.get_debug()
                );
                true
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

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
#[macro_use]
extern crate structopt_derive;
extern crate tokio_core;
extern crate tokio_signal;
extern crate tsproto;

use std::cell::RefCell;
use std::sync::Mutex;
use std::net::SocketAddr;
use std::rc::{Rc, Weak};

use futures::{future, Future, Sink, Stream};
use futures::sync::mpsc;
use gst::prelude::*;
use gst_audio::StreamVolumeExt;
use num_traits::cast::ToPrimitive;
use slog::{Drain, Logger};
use structopt::StructOpt;
use structopt::clap::AppSettings;
use tokio_core::reactor::{Core, Handle};
use tokio_signal::unix::{Signal, SIGHUP};
use tsproto::*;
use tsproto::client::ClientData;
use tsproto::packets::*;

mod utils;
use utils::*;
use utils::voice::IncommingVoiceHandler2;

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
                help = "The URI of the audio, which should be played. If it is empty, it will capture from a microphone.")]
    uri: String,
    #[structopt(long = "volume", short = "v", default_value = "0.0",
                help = "The volume of audio-to-ts.")]
    volume: f64,
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

    // Setup incoming voice handler
    let in_pipe = create_ts_to_audio_pipeline(logger.clone()).unwrap();
    let audio = main_loop(&in_pipe, logger.clone()).unwrap();
    in_pipe.set_state(gst::State::Playing).into_result().unwrap();
    let in_handler = IncommingVoiceHandler2::new(logger.clone(), in_pipe).unwrap();
    ClientData::apply_packet_stream_wrapper::<IncommingVoiceHandler2>(&c, in_handler);

    // Run event handler in background
    handle.spawn(audio);

    // Connect
    if let Err(error) = core.run(connect(logger.clone(), &handle, c.clone(),
        args.address)) {
        error!(logger, "Failed to connect"; "error" => ?error);
        return;
    }
    info!(logger, "Connected");

    // Disconnect if audio fails
    let (pipeline, audio) = match setup_audio(&args, Rc::downgrade(&c), handle.clone(), logger.clone()) {
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

    // Wait until song is finished
    //if let Err(_error) = core.run(audio) {
        // Also returns with an error when the stream finished
        //error!(logger, "Error while playing"; "error" => ?error);
    //}
    //info!(logger, "Waited");

    handle.spawn(audio);

    // Pause or unpause sending on sighup
    let pipe = pipeline.clone();
    let logger2 = logger.clone();
    let logger3 = logger.clone();
    let sighup = Signal::new(SIGHUP, &handle).flatten_stream().for_each(move |_| {
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

    // Stop with ctrl + c
    let ctrl_c = tokio_signal::ctrl_c(&handle).flatten_stream();
    core.run(ctrl_c.into_future().map(move |_| ()).map_err(move |_| ())).unwrap();

    // Disconnect
    if let Err(error) = core.run(disconnect(c.clone(), args.address)) {
        error!(logger, "Failed to disconnect"; "error" => ?error);
        return;
    }
    info!(logger, "Disconnected");

    // Cleanup gstreamer
    pipeline.set_state(gst::State::Null).into_result().unwrap();
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

fn create_ts_to_audio_pipeline(logger: Logger) -> Result<gst::Pipeline, failure::Error> {
    let pipeline = gst::Pipeline::new("ts-to-audio-pipeline");

    let appsrc = gst::ElementFactory::make("appsrc", "appsrc").ok_or_else(||
        format_err!("Missing appsrc"))?;
    let demuxer = gst::ElementFactory::make("ts3audiodemux", "demuxer").ok_or_else(||
        format_err!("Missing ts3audiodemux"))?;

    let mixer = gst::ElementFactory::make("audiomixer", "mixer").ok_or_else(||
        format_err!("Missing audiomixer"))?;
    // Maybe force to resample to 48 kHz before and to hardware now.
    // This would make all resamplers before just pass-through for opus which
    // already uses 48 kHz.
    /*let resampler = gst::ElementFactory::make("audioresample", None).ok_or_else(||
        format_err!("Missing audioresample"))?;*/
    let audioconvert = gst::ElementFactory::make("audioconvert", None).ok_or_else(||
        format_err!("Missing audioconvert"))?;
    let autosink = gst::ElementFactory::make("autoaudiosink", None).ok_or_else(||
        format_err!("Missing autoaudiosink"))?;

    let src = appsrc.clone().dynamic_cast::<gst_app::AppSrc>().unwrap();
    src.set_caps(&gst::Caps::new_simple("audio/x-ts3audio", &[]));
    src.set_property_format(gst::Format::Time);
    //src.set_property("blocksize", &::glib::Value::from(&500u32))?;
    //src.set_max_bytes(1500);
    //src.set_property_min_latency((gst::SECOND_VAL / 50) as i64); // 20 ms in ns
    //src.set_property_min_latency((gst::SECOND_VAL / 30) as i64); // 20 ms in ns
    src.set_property_min_latency(0); // in ns
    // Important to reduce the playback latency
    src.set_property("do-timestamp", &::glib::Value::from(&true))?;
    // Set as live source, which means it does not produce data when paused
    src.set_property("is-live", &::glib::Value::from(&true))?;
    // TODO There is latency again for some reason

    let fakesrc = gst::ElementFactory::make("audiotestsrc", "fake").ok_or_else(||
        format_err!("Missing audiotestsrc"))?;
    fakesrc.set_property("volume", &::glib::Value::from(&0f64))?;
    fakesrc.set_property("do-timestamp", &::glib::Value::from(&true))?;
    fakesrc.set_property("is-live", &::glib::Value::from(&true))?;

    pipeline.add_many(&[&appsrc, &demuxer, &mixer, &audioconvert, &autosink, &fakesrc])?;
    gst::Element::link_many(&[&mixer, &audioconvert, &autosink])?;
    gst::Element::link_many(&[&appsrc, &demuxer])?;
    fakesrc.link_filtered(&mixer, &gst::Caps::new_simple(
        "audio/x-raw", &[("rate", &48000i32), ("channels", &2i32)]))?;


    let pipe = pipeline.clone();
    let mix = mixer.clone();
    // Link demuxer to mixer when a new client speaks
    demuxer.connect_pad_added(move |demuxer, src_pad| {
        debug!(logger, "Got new client pad"; "name" => src_pad.get_name());
        // Create decoder
        // TODO Create fitting decoder and not decodebin
        //let decode = gst::ElementFactory::make("opusdec",
        let decode = gst::ElementFactory::make("decodebin",
            format!("decoder_{}", src_pad.get_name()).as_str())
            .expect("Missing decodebin");
        //pipe.set_state(gst::State::Paused).into_result().unwrap();
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
        let mixer = mix.clone();
        let decoder = decode.clone();
        let demuxer = demuxer.clone();
        let pipe = pipe.clone();
        decode.connect_pad_added(move |dbin, src_pad| {
            debug!(logger, "Got new client decoder pad"; "name" => src_pad.get_name());

            // Link to sink pad of next element
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

            // Add eos probe
            let decode = decoder.clone();
            let logger = logger.clone();
            let mixer = mixer.clone();
            let demuxer = demuxer.clone();
            let pipe = pipe.clone();
            sink_pad.add_probe(gst::PadProbeType::EVENT_DOWNSTREAM, move |_pad, info| {
                let eos = info.data.as_ref()
                    .and_then(|d| if let gst::PadProbeData::Event(e) = d {
                        Some(e.get_type() == gst::EventType::Eos)
                    } else {
                        None
                    })
                    .unwrap_or(false);

                if eos {
                    // Unlink and remove decoder
                    debug!(logger, "Remove client decoder");

                    let decode_src_pad = decode.iterate_src_pads().next();
                    let mixer_pad = decode_src_pad.and_then(|p| p.ok())
                        .and_then(|p| p.get_peer());
                    let decode_sink_pad = decode.iterate_sink_pads().next();
                    let demuxer_pad = decode_sink_pad.and_then(|p| p.ok())
                        .and_then(|p| p.get_peer());

                    gst::Element::unlink_many(&[&demuxer, &decode, &mixer]);

                    // Remove pad from mixer
                    if let Some(pad) = mixer_pad {
                        if let Err(e) = mixer.remove_pad(&pad) {
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

                    if let Err(e) = pipe.remove(&decode) {
                        error!(logger, "Cannot remove decoder from pipeline"; "error" => ?e);
                    }
                    // Cleanup
                    decode.set_state(gst::State::Null).into_result().unwrap();
                }

                gst::PadProbeReturn::Ok
            });
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

    // On the receiving side: rtpjitterbuffer do-lost=true,

    opusenc.set_property_from_str("bitrate-type", "vbr");
    opusenc.set_property_from_str("audio-type", "generic"); // or voice
    // Discontinuous transmission: Reduce bandwidth of silence
    opusenc.set_property("dtx", &glib::Value::from(&true))?;
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
            MessageView::Qos(_) => {
                debug!(logger, "GOT QOS");
                false
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

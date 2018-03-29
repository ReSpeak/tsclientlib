use std::net::SocketAddr;
use std::sync::Mutex;

use {gst, gst_app};
use futures::Stream;
use gst::prelude::*;
use num_traits::FromPrimitive;
use slog::Logger;
use tsproto::*;
use tsproto::packets::*;

pub struct IncommingVoiceHandler {
    pub logger: Logger,
    pub pipeline: gst::Pipeline,
}

impl Drop for IncommingVoiceHandler {
    fn drop(&mut self) {
        // Cleanup gstreamer
        self.pipeline.set_state(gst::State::Null).into_result().unwrap();
    }
}

impl IncommingVoiceHandler {
    fn handle_packet(&mut self, addr: SocketAddr, packet: &Packet)
        -> Result<(), ::failure::Error> {
        // Check if we got a voice packet
        if let packets::Data::VoiceS2C { from_id, codec_type, ref voice_data, .. } = packet.data {
            let codec = if let Some(codec) = CodecType::from_u8(codec_type) {
                codec
            } else {
                return Err(format_err!("Cannot parse codec {}", codec_type).into());
            };
            // Check if there is already a source for this combination
            let id = format!("Clientsrc-{}-{}-{:?}", addr, from_id, codec);
            let appsrc = if let Some(src) = self.pipeline.get_by_name(&id) {
                // Ensure that the pipeline is playing
                //self.pipeline.set_state(gst::State::Paused).into_result()?;
                //self.pipeline.set_state(gst::State::Playing).into_result()?;
                //src.set_state(gst::State::Playing).into_result()?;
                src.dynamic_cast::<gst_app::AppSrc>().unwrap()
            } else {
                // Create a new source
                if codec == CodecType::OpusVoice || codec == CodecType::OpusMusic {
                    let source = gst::ElementFactory::make("appsrc", id.as_str()).ok_or_else(||
                        format_err!("Missing appsrc"))?;

                    let opusdec = gst::ElementFactory::make("opusdec",
                        format!("dec-{}", id).as_str()).ok_or_else(||
                        format_err!("Missing opusdec"))?;
                    let resampler = gst::ElementFactory::make("audioresample",
                        format!("resample-{}", id).as_str()).ok_or_else(||
                        format_err!("Missing audioresample"))?;

                    // Set capabilities
                    let src = source.clone().dynamic_cast::<gst_app::AppSrc>().unwrap();
                    src.set_caps(&gst::Caps::new_simple("audio/x-opus",
                        &[("channel-mapping-family", &0i32)]));

                    src.set_property_format(gst::Format::Time);
                    //src.set_property("blocksize", &::glib::Value::from(&500u32))?;
                    //src.set_property_is_live(true);
                    src.set_max_bytes(1000); // About two packets
                    // TODO Set to ping deviation?
                    src.set_property_min_latency(0); // in ns
                    src.set_property_max_latency(1000000); // in ns
                    //src.set_property("do-timestamp", &::glib::Value::from(&true))?;

                    {
                        let elements = [&source, &opusdec, &resampler];
                        self.pipeline.add_many(&elements)?;
                        gst::Element::link_many(&elements)?;

                        for e in &elements {
                            e.sync_state_with_parent()?;
                        }
                    }

                    // Request new mixer pad
                    let mixer = self.pipeline.get_by_name("mixer").ok_or_else(||
                        format_err!("Mixer not found"))?;
                    let sink_pad = mixer.get_request_pad("sink_%u")
                        .ok_or_else(|| format_err!("Failed to request pad"))?;
                    let source_pad = resampler.get_static_pad("src").unwrap();
                    source_pad.link(&sink_pad).into_result()?;


                    let pipeline = self.pipeline.clone();
                    let logger = self.logger.clone();
                    let id2 = id.clone();
                    let is_first = Mutex::new(0);
                    src.connect_need_data(move |_, _| {
                        // Remove source on underflow (client stopped talking)
                        if let Err(error) = (|| -> Result<(), ::failure::Error> {
                            info!(logger, "Need data"; "id" => &id2);
                            pipeline.set_state(gst::State::Paused).into_result()?;
                            let mut is_first = is_first.lock().unwrap();
                            if *is_first <= 100 {
                                *is_first += 1;
                            } else {
                                info!(logger, "Removed source"; "id" => &id2);
                                pipeline.remove_many(&[&source, &opusdec, &resampler])?;
                            }
                            Ok(())
                        })() {
                            error!(logger, "Error when removing client sink";
                                "error" => ?error, "id" => &id2);
                        }
                    });

                    let pipeline = self.pipeline.clone();
                    let logger = self.logger.clone();
                    let id2 = id.clone();
                    src.connect_enough_data(move |_| {
                        // Remove source on underflow (client stopped talking)
                        if let Err(error) = (|| -> Result<(), ::failure::Error> {
                            info!(logger, "Enough data"; "id" => &id2);
                            pipeline.set_state(gst::State::Playing).into_result()?;
                            Ok(())
                        })() {
                            error!(logger, "Error when removing client sink";
                                "error" => ?error, "id" => &id2);
                        }
                    });


                    info!(self.logger, "Added new source"; "id" => id);
                    self.pipeline.set_state(gst::State::Paused).into_result()?;
                    src
                } else {
                    warn!(self.logger, "Unsupported codec"; "codec" => ?codec);
                    return Ok(());
                }
            };
            let mut buffer = gst::Buffer::with_size(voice_data.len()).unwrap();

            {
                let buffer = buffer.get_mut().unwrap();
                let clock = self.pipeline.get_pipeline_clock().unwrap();
                let timestamp = clock.get_time();
                //buffer.set_pts(timestamp /*+ gst::ClockTime::from_mseconds(1)*/);
                let mut data = buffer.map_writable().unwrap();

                data.copy_from_slice(voice_data);
            }

            let res = appsrc.push_buffer(buffer);
            if res != gst::FlowReturn::Ok {
                error!(self.logger, "Failed to push buffer to source"; "result" => ?res);
            }
        }
        Ok(())
    }
}

impl<Inner: Stream<Item = (SocketAddr, Packet), Error = Error> + 'static>
    StreamWrapper<(SocketAddr, Packet), Error, Inner> for IncommingVoiceHandler {
    type A = IncommingVoiceHandler;
    type Result = Box<Stream<Item = (SocketAddr, Packet), Error = Error>>;

    fn wrap(inner: Inner, mut handler: Self::A) -> Self::Result {
        Box::new(inner.inspect(move |item: &(SocketAddr, Packet)| {
            if let Err(error) = handler.handle_packet(item.0, &item.1) {
                error!(handler.logger, "Error handling voice packet";
                    "error" => ?error);
            }
        }))
    }
}

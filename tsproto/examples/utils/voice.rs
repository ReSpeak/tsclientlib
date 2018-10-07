use std::io::Cursor;

use {gst, gst_app};
use gst::prelude::*;
use slog::Logger;
use tsproto::*;
use tsproto::packets::*;

#[derive(Clone)]
pub struct IncommingVoiceHandler {
    logger: Logger,
    pipeline: Option<gst::Pipeline>,
    appsrc: Option<gst_app::AppSrc>,
}

impl Drop for IncommingVoiceHandler {
    fn drop(&mut self) {
        if let Some(pipeline) = &self.pipeline {
            // Cleanup gstreamer
            pipeline.set_state(gst::State::Null).into_result().unwrap();
        }
    }
}

// Only used in some examples
#[allow(dead_code)]
impl IncommingVoiceHandler {
    pub fn new(logger: Logger, pipeline: Option<gst::Pipeline>) -> Result<Self, ::failure::Error> {
        let appsrc = pipeline.as_ref().map(|p| {
            let appsrc = p.get_by_name("appsrc").expect("AppSrc not found");
            appsrc.dynamic_cast::<gst_app::AppSrc>().unwrap()
        });
        Ok(Self {
            logger,
            pipeline,
            appsrc,
        })
    }

    pub fn handle_packet(&mut self, packet: &Packet) -> Result<(), ::failure::Error> {
        if let Some(appsrc) = &self.appsrc {
            // Check if we got a voice packet
            if let packets::Data::VoiceS2C { voice_data, .. } = &packet.data {
                let mut buffer = gst::Buffer::with_size(voice_data.len() + 5).unwrap();

                {
                    let buffer = buffer.get_mut().unwrap();

                    //let clock = self.appsrc.get_clock().unwrap();
                    //println!("Push buffer {} at {}", id, clock.get_time() - self.appsrc.get_base_time());

                    let mut data = buffer.map_writable().unwrap();
                    packet.data.write(&mut Cursor::new(&mut *data))?;
                }

                let res = appsrc.push_buffer(buffer);
                if res != gst::FlowReturn::Ok {
                    error!(self.logger, "Failed to push buffer to source"; "result" => ?res);
                }
            }
        }
        Ok(())
    }
}

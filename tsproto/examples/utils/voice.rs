use std::io::Cursor;
use std::net::SocketAddr;

use {gst, gst_app};
use futures::Stream;
use gst::prelude::*;
use slog::Logger;
use tsproto::*;
use tsproto::packets::*;

pub struct IncommingVoiceHandler {
    logger: Logger,
    pipeline: gst::Pipeline,
    appsrc: gst_app::AppSrc,
}

impl Drop for IncommingVoiceHandler {
    fn drop(&mut self) {
        // Cleanup gstreamer
        self.pipeline.set_state(gst::State::Null).into_result().unwrap();
    }
}

impl IncommingVoiceHandler {
    pub fn new(logger: Logger, pipeline: gst::Pipeline) -> Result<Self, ::failure::Error> {
        let appsrc = pipeline.get_by_name("appsrc").expect("AppSrc not found");
        let appsrc = appsrc.dynamic_cast::<gst_app::AppSrc>().unwrap();
        Ok(Self {
            logger,
            pipeline,
            appsrc,
        })
    }

    fn handle_packet(&mut self, _addr: SocketAddr, packet: &Packet)
        -> Result<(), ::failure::Error> {
        // Check if we got a voice packet
        if let packets::Data::VoiceS2C { ref voice_data, id, .. } = packet.data {
            let mut buffer = gst::Buffer::with_size(voice_data.len() + 5).unwrap();

            {
                let buffer = buffer.get_mut().unwrap();

                let clock = self.appsrc.get_clock().unwrap();
                //println!("Push buffer {} at {}", id, clock.get_time() - self.appsrc.get_base_time());

                let mut data = buffer.map_writable().unwrap();
                packet.data.write(&mut Cursor::new(&mut *data))?;
            }

            let res = self.appsrc.push_buffer(buffer);
            if res != gst::FlowReturn::Ok {
                error!(self.logger, "Failed to push buffer to source"; "result" => ?res);
            }
        }
        Ok(())
    }
}
// TODO use

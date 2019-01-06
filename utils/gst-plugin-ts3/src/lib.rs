// Partially copied from https://github.com/sdroege/gst-plugin-rs

extern crate glib;
#[macro_use]
extern crate gst_plugin;
#[macro_use]
extern crate gstreamer as gst;
extern crate gstreamer_base as gst_base;

mod demuxer;

fn plugin_init(plugin: &gst::Plugin) -> bool {
	demuxer::register(
		plugin,
		demuxer::DemuxerInfo {
			name: "ts3audiodemux".into(),
			long_name: "TeamSpeak 3 Audio Demuxer".into(),
			description: "Demux TeamSpeak 3 Audio Packets".into(),
			classification: "Codec/Demuxer".into(),
			author: "Flakebi <flakebi@t-online.de>".into(),
			rank: 256 + 100,
			input_caps: gst::Caps::new_simple("audio/x-ts3audio", &[]),
			output_caps: gst::Caps::new_any(),
		},
	);
	true
}

plugin_define!(
	b"ts3\0",
	b"TeamSpeak 3 Plugin\0",
	plugin_init,
	b"0.1.0\0",
	b"MIT/X11\0",
	b"ts3\0",
	b"ts3\0",
	b"https://github.com/Flakebi/gst-plugin-ts3\0",
	b"2018-08-21\0"
);

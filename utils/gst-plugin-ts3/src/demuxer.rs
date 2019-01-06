use std::collections::BTreeMap;
use std::io::{Cursor, Write};
use std::sync::Mutex;
use std::u32;

use gst_plugin::bytes::{WriteBytesExt, WriteBytesExtShort};
use gst_plugin::element::*;
use gst_plugin::object::*;

use gst;
use gst::prelude::*;
use gst_base;

pub struct DemuxerInfo {
	pub name: String,
	pub long_name: String,
	pub description: String,
	pub classification: String,
	pub author: String,
	pub rank: u32,
	pub input_caps: gst::Caps,
	pub output_caps: gst::Caps,
}

pub struct Demuxer {
	cat: gst::DebugCategory,
	sinkpad: gst::Pad,
	flow_combiner: Mutex<UniqueFlowCombiner>,
	group_id: Mutex<gst::GroupId>,
	srcpads: Mutex<BTreeMap<u32, gst::Pad>>,
}

#[derive(Default)]
pub struct UniqueFlowCombiner(gst_base::FlowCombiner);

#[derive(Debug, PartialEq, Eq, Clone)]
enum Codec {
	Speex,
	Celt,
	Opus,
}

#[derive(Debug, Eq, Clone)]
struct AudioFormat {
	codec: Codec,
	rate: u16,
	width: u8,
	channels: u8,
	bitrate: Option<u32>,
}

// Ignores bitrate
impl PartialEq for AudioFormat {
	fn eq(&self, other: &Self) -> bool {
		self.codec.eq(&other.codec)
			&& self.rate.eq(&other.rate)
			&& self.width.eq(&other.width)
			&& self.channels.eq(&other.channels)
	}
}

impl AudioFormat {
	fn new_data(codec: Codec, rate: u16, channels: u8) -> Self {
		Self {
			codec,
			rate,
			width: 2,
			channels,
			bitrate: None,
		}
	}

	fn new(typ: u8) -> Option<Self> {
		Some(match typ {
			0 => Self::new_data(Codec::Speex, 8_000, 1),
			1 => Self::new_data(Codec::Speex, 16_000, 1),
			2 => Self::new_data(Codec::Speex, 32_000, 1),
			3 => Self::new_data(Codec::Celt, 48_000, 1),
			4 => Self::new_data(Codec::Opus, 48_000, 1),
			5 => Self::new_data(Codec::Opus, 48_000, 2),
			_ => return None,
		})
	}

	fn to_caps(&self) -> gst::Caps {
		let mut caps = match self.codec {
			Codec::Speex => {
				// TODO Speex does not work
				let header = {
					let header_size = 80;
					let mut data = Cursor::new(Vec::with_capacity(header_size));
					data.write_all(b"Speex   1.1.12").unwrap();
					data.write_all(&[0; 14]).unwrap();
					data.write_u32le(1).unwrap(); // version
					data.write_u32le(80).unwrap(); // header size
					data.write_u32le(16_000).unwrap(); // sample rate
										// TODO Not only wideband
					data.write_u32le(1).unwrap(); // mode = wideband
					data.write_u32le(4).unwrap(); // mode bitstream version
					data.write_u32le(1).unwrap(); // channels
					data.write_i32le(-1).unwrap(); // bitrate
					data.write_u32le(16_000 / 50 * 2).unwrap(); // frame size
					data.write_u32le(0).unwrap(); // VBR
					data.write_u32le(1).unwrap(); // frames per packet
					data.write_u32le(0).unwrap(); // extra headers
					data.write_u32le(0).unwrap(); // reserved 1
					data.write_u32le(0).unwrap(); // reserved 2

					assert_eq!(data.position() as usize, header_size);

					data.into_inner()
				};
				let header = gst::Buffer::from_mut_slice(header).unwrap();

				let comment = {
					let comment_size = 4 + 7 /* nothing */ + 4 + 1;
					let mut data =
						Cursor::new(Vec::with_capacity(comment_size));
					data.write_u32le(7).unwrap(); // length of "nothing"
					data.write_all(b"nothing").unwrap(); // "vendor" string
					data.write_u32le(0).unwrap(); // number of elements
					data.write_u8(1).unwrap();

					assert_eq!(data.position() as usize, comment_size);

					data.into_inner()
				};
				let comment = gst::Buffer::from_mut_slice(comment).unwrap();

				gst::Caps::new_simple(
					"audio/x-speex",
					&[("streamheader", &gst::Array::new(&[&header, &comment]))],
				)
			}
			Codec::Celt => {
				// TODO celt is no opus, so the celt encoding was removed from
				// gstreamer. The version which ts uses seems older.
				//gst::Caps::new_simple("audio/x-celt", &[])
				gst::Caps::new_simple(
					"audio/x-opus",
					&[("channel-mapping-family", &0i32)],
				)
			}
			Codec::Opus => gst::Caps::new_simple(
				"audio/x-opus",
				&[("channel-mapping-family", &0i32)],
			),
		};

		if self.rate != 0 {
			caps.get_mut()
				.unwrap()
				.set_simple(&[("rate", &(self.rate as i32))])
		}
		if self.channels != 0 {
			caps.get_mut()
				.unwrap()
				.set_simple(&[("channels", &(self.channels as i32))])
		}

		caps
	}
}

impl UniqueFlowCombiner {
	fn add_pad(&mut self, pad: &gst::Pad) {
		self.0.add_pad(pad);
	}

	fn remove_pad(&mut self, pad: &gst::Pad) {
		self.0.remove_pad(pad);
	}

	fn clear(&mut self) {
		self.0.clear();
	}

	fn update_flow(&mut self, flow_ret: gst::FlowReturn) -> gst::FlowReturn {
		self.0.update_flow(flow_ret)
	}
}

unsafe impl Send for UniqueFlowCombiner {}

impl Demuxer {
	fn new(sinkpad: gst::Pad, info: &DemuxerInfo) -> Self {
		Self {
			cat: gst::DebugCategory::new(
				info.name.as_str(),
				gst::DebugColorFlags::empty(),
				info.long_name.as_str(),
			),
			sinkpad: sinkpad,
			flow_combiner: Mutex::new(Default::default()),
			group_id: Mutex::new(gst::util_group_id_next()),
			srcpads: Mutex::new(BTreeMap::new()),
		}
	}

	fn class_init(klass: &mut ElementClass, info: &DemuxerInfo) {
		klass.set_metadata(
			&info.long_name,
			&info.classification,
			&info.description,
			&info.author,
		);

		let pad_template = gst::PadTemplate::new(
			"sink",
			gst::PadDirection::Sink,
			gst::PadPresence::Always,
			&info.input_caps,
		);
		klass.add_pad_template(pad_template);

		let pad_template = gst::PadTemplate::new(
			"src_%u",
			gst::PadDirection::Src,
			gst::PadPresence::Sometimes,
			&info.output_caps,
		);
		klass.add_pad_template(pad_template);
	}

	fn init(
		element: &Element,
		info: &DemuxerInfo,
	) -> Box<ElementImpl<Element>> {
		let templ = element.get_pad_template("sink").unwrap();
		let sinkpad = gst::Pad::new_from_template(&templ, "sink");
		sinkpad.set_activate_function(Demuxer::sink_activate);
		sinkpad.set_activatemode_function(Demuxer::sink_activatemode);
		sinkpad.set_chain_function(Demuxer::sink_chain);
		sinkpad.set_event_function(Demuxer::sink_event);
		element.add_pad(&sinkpad).unwrap();

		let imp = Self::new(sinkpad, info);
		element.connect_pad_removed(move |element, pad| {
			let demuxer = element.get_impl().downcast_ref::<Demuxer>().unwrap();
			let pad_index = {
				let srcpads = demuxer.srcpads.lock().unwrap();
				let mut index = None;
				for (&i, p) in srcpads.iter() {
					if p == pad {
						index = Some(i);
						break;
					}
				}
				if let Some(i) = index {
					i
				} else {
					return;
				}
			};
			demuxer.remove_stream(element, pad_index);
		});
		Box::new(imp)
	}

	pub fn add_stream(
		&self,
		element: &Element,
		index: u32,
		caps: gst::Caps,
		stream_id: &str,
	) {
		let mut srcpads = self.srcpads.lock().unwrap();
		assert!(!srcpads.contains_key(&index));

		let templ = element.get_pad_template("src_%u").unwrap();
		let name = format!("src_{}", index);
		let pad = gst::Pad::new_from_template(&templ, Some(name.as_str()));
		pad.set_query_function(Demuxer::src_query);
		pad.set_event_function(Demuxer::src_event);

		pad.set_active(true).unwrap();

		let full_stream_id = pad.create_stream_id(element, stream_id).unwrap();
		pad.push_event(
			gst::Event::new_stream_start(&full_stream_id)
				.group_id(*self.group_id.lock().unwrap())
				.build(),
		);
		pad.push_event(gst::Event::new_caps(&caps).build());

		let segment = gst::FormattedSegment::<gst::ClockTime>::default();
		pad.push_event(gst::Event::new_segment(&segment).build());

		self.flow_combiner.lock().unwrap().add_pad(&pad);
		element.add_pad(&pad).unwrap();

		srcpads.insert(index, pad);
	}

	pub fn stream_eos(&self, _element: &Element, index: Option<u32>) {
		let srcpads = self.srcpads.lock().unwrap();

		let event = gst::Event::new_eos().build();
		match index {
			Some(index) => if let Some(pad) = srcpads.get(&index) {
				pad.push_event(event);
			},
			None => for (_, pad) in srcpads.iter().by_ref() {
				pad.push_event(event.clone());
			},
		};
	}

	pub fn stream_push_buffer(
		&self,
		index: u32,
		buffer: gst::Buffer,
	) -> gst::FlowReturn {
		let srcpads = self.srcpads.lock().unwrap();

		if let Some(pad) = srcpads.get(&index) {
			let res = pad.push(buffer);
			self.flow_combiner.lock().unwrap().update_flow(res)
		} else {
			gst::FlowReturn::Error
		}
	}

	fn remove_all_streams(&self, element: &Element) {
		self.flow_combiner.lock().unwrap().clear();
		let mut srcpads = self.srcpads.lock().unwrap();
		for (_, pad) in srcpads.iter().by_ref() {
			element.remove_pad(pad).unwrap();
		}
		srcpads.clear();
	}

	pub fn remove_stream(&self, element: &Element, index: u32) {
		let pad = {
			let mut srcpads = self.srcpads.lock().unwrap();
			if let Some(pad) = srcpads.remove(&index) {
				self.flow_combiner.lock().unwrap().remove_pad(&pad);
				Some(pad)
			} else {
				None
			}
		};
		if let Some(pad) = pad {
			// Ignore if the pad is already removed
			let _ = element.remove_pad(&pad);
		}
	}

	fn sink_activate(pad: &gst::Pad, _parent: &Option<gst::Object>) -> bool {
		let mode = {
			let mut query = gst::Query::new_scheduling();
			if !pad.peer_query(&mut query) {
				return false;
			}

			// TODO
			//if (gst_query_has_scheduling_mode_with_flags (query, GST_PAD_MODE_PULL, GST_SCHEDULING_FLAG_SEEKABLE)) {
			//  GST_DEBUG_OBJECT (demuxer, "Activating in PULL mode");
			//  mode = GST_PAD_MODE_PULL;
			//} else {
			//GST_DEBUG_OBJECT (demuxer, "Activating in PUSH mode");
			//}
			gst::PadMode::Push
		};

		match pad.activate_mode(mode, true) {
			Ok(_) => true,
			Err(_) => false,
		}
	}

	fn sink_activatemode(
		_pad: &gst::Pad,
		parent: &Option<gst::Object>,
		mode: gst::PadMode,
		active: bool,
	) -> bool {
		let element = parent
			.as_ref()
			.cloned()
			.unwrap()
			.downcast::<Element>()
			.unwrap();
		let demuxer = element.get_impl().downcast_ref::<Demuxer>().unwrap();

		if active {
			let upstream_size = demuxer
				.sinkpad
				.peer_query_duration::<gst::format::Bytes>()
				.and_then(|v| v.0);

			gst_debug!(
				demuxer.cat,
				obj: &element,
				"Starting with upstream size {:?}",
				upstream_size,
			);

			if mode == gst::PadMode::Pull {
				// TODO
				// demuxer.sinkpad.start_task(...)
			}

			true
		} else {
			if mode == gst::PadMode::Pull {
				let _ = demuxer.sinkpad.stop_task();
			}

			gst_debug!(demuxer.cat, obj: &element, "Stopping");
			true
		}
	}

	fn sink_chain(
		_pad: &gst::Pad,
		parent: &Option<gst::Object>,
		buffer: gst::Buffer,
	) -> gst::FlowReturn {
		let element = parent
			.as_ref()
			.cloned()
			.unwrap()
			.downcast::<Element>()
			.unwrap();
		let demuxer = element.get_impl().downcast_ref::<Demuxer>().unwrap();

		// Buffer content:
		// 2 byte packet id
		// 2 byte client id
		// 1 byte codec type
		// voice data

		// Codec type:
		// SpeexNarrowband, Mono, 16bit, 8kHz
		// SpeexWideband, Mono, 16bit, 16kHz
		// SpeexUltraWideband, Mono, 16bit, 32kHz
		// CeltMono, Mono, 16bit, 48kHz
		// OpusVoice, Mono, 16bit, 48kHz, optimized for voice
		// OpusMusic, Stereo, 16bit, 48kHz, optimized for music

		{
			let demuxer = element.get_impl().downcast_ref::<Demuxer>().unwrap();
			let stream_index;
			if let Some(map) = buffer.map_readable() {
				// Find target client and codec
				let client_id = ((map[2] as u16) << 8) | map[3] as u16;
				let format = if let Some(format) = AudioFormat::new(map[4]) {
					format
				} else {
					gst_error!(
						demuxer.cat,
						obj: &element,
						"Unsupported audio format: {}",
						map[4],
					);
					return gst::FlowReturn::Ok;
				};
				gst_trace!(
					demuxer.cat,
					obj: &element,
					"Handling buffer {:?} from {} {:?}",
					buffer,
					client_id,
					format.codec
				);

				stream_index = ((client_id as u32) << 8) | map[4] as u32;
				let stream_id = format!("src_{}_{:?}", client_id, format.codec);

				// End of stream
				if map.len() < 7 {
					gst_debug!(
						demuxer.cat,
						obj: &element,
						"Got empty audio packet {}",
						stream_index
					);
					return gst::FlowReturn::Ok;
				}

				if !demuxer.has_src_pad(stream_index) {
					gst_debug!(
						demuxer.cat,
						obj: &element,
						"Got new stream {}",
						stream_index
					);
					demuxer.add_stream(
						&element,
						stream_index,
						format.to_caps(),
						&stream_id,
					);
				}
			} else {
				gst_error!(demuxer.cat, obj: &element, "Failed to map buffer");
				return gst::FlowReturn::Error;
			}

			// Cut off metadata
			let mut buffer2 = buffer
				.copy_region(
					gst::BufferCopyFlags::all(),
					5,
					Some(buffer.get_size() - 5),
				).unwrap();
			{
				let buffer2 = buffer2.get_mut().unwrap();
				let mut flags = buffer.get_flags();
				flags.insert(gst::BufferFlags::LIVE);
				//buffer2.set_flags(flags);

				buffer2.set_pts(buffer.get_pts());
			}

			demuxer.stream_push_buffer(stream_index, buffer2)
		}
	}

	fn sink_event(
		pad: &gst::Pad,
		parent: &Option<gst::Object>,
		event: gst::Event,
	) -> bool {
		use gst::EventView;

		let element = parent
			.as_ref()
			.cloned()
			.unwrap()
			.downcast::<Element>()
			.unwrap();
		let demuxer = element.get_impl().downcast_ref::<Demuxer>().unwrap();

		// TODO Remove pad when pad gets removed by user
		match event.view() {
			EventView::Eos(..) => {
				gst_debug!(demuxer.cat, obj: &element, "End of stream");
				pad.event_default(parent.as_ref(), event)
			}
			EventView::Segment(..) => pad.event_default(parent.as_ref(), event),
			_ => pad.event_default(parent.as_ref(), event),
		}
	}

	fn src_query(
		pad: &gst::Pad,
		parent: &Option<gst::Object>,
		query: &mut gst::QueryRef,
	) -> bool {
		use gst::QueryView;

		let element = parent
			.as_ref()
			.cloned()
			.unwrap()
			.downcast::<Element>()
			.unwrap();
		let demuxer = element.get_impl().downcast_ref::<Demuxer>().unwrap();

		match query.view_mut() {
			QueryView::Position(ref mut q) => {
				let fmt = q.get_format();
				if fmt == gst::Format::Time {
					let position = 0; //demuxer_impl.get_position(&element);
					gst_trace!(
						demuxer.cat,
						obj: &element,
						"Returning position {:?}",
						position
					);

				/*match *position {
						None => return false,
						Some(_) => {
							q.set(position);
							return true;
						}
					}*/
				} else {
					return false;
				}
			}
			QueryView::Duration(ref mut q) => {
				let fmt = q.get_format();
				if fmt == gst::Format::Time {
					let duration = 0; //demuxer_impl.get_duration(&element);
					gst_trace!(
						demuxer.cat,
						obj: &element,
						"Returning duration {:?}",
						duration
					);

				/*match *duration {
						None => return false,
						Some(_) => {
							q.set(duration);
							return true;
						}
					}*/
				} else {
					return false;
				}
			}
			_ => (),
		}

		// FIXME: Have to do it outside the match because otherwise query is already mutably
		// borrowed by the query view.
		pad.query_default(parent.as_ref(), query)
	}

	fn src_event(
		pad: &gst::Pad,
		parent: &Option<gst::Object>,
		event: gst::Event,
	) -> bool {
		use gst::EventView;

		/*match event.view() {
			EventView::Latency(l) => {
				let element = parent
					.as_ref()
					.cloned()
					.unwrap()
					.downcast::<Element>()
					.unwrap();
				let demuxer = element.get_impl().downcast_ref::<Demuxer>().unwrap();
				let demuxer_impl = &mut demuxer.imp.lock().unwrap();
				demuxer_impl.set_src_latency(l.get_latency());
				return true;
			}
			_ => {}
		}*/
		pad.event_default(parent.as_ref(), event)
	}

	pub fn has_src_pad(&self, id: u32) -> bool {
		let pads = self.srcpads.lock().unwrap();
		pads.contains_key(&id)
	}

	pub fn get_src_pad(&self, id: u32) -> Option<gst::Pad> {
		let pads = self.srcpads.lock().unwrap();
		pads.get(&id).map(Clone::clone)
	}
}

impl ObjectImpl<Element> for Demuxer {}

impl ElementImpl<Element> for Demuxer {
	fn change_state(
		&self,
		element: &Element,
		transition: gst::StateChange,
	) -> gst::StateChangeReturn {
		gst_trace!(self.cat, obj: element, "Changing state {:?}", transition);

		match transition {
			gst::StateChange::ReadyToPaused => {
				// TODO
				*self.group_id.lock().unwrap() = gst::util_group_id_next();
			}
			_ => (),
		}

		let ret = element.parent_change_state(transition);
		if ret == gst::StateChangeReturn::Failure {
			return ret;
		}

		match transition {
			gst::StateChange::PausedToReady => {
				self.flow_combiner.lock().unwrap().clear();
				let mut srcpads = ::std::mem::replace(
					&mut *self.srcpads.lock().unwrap(),
					Default::default(),
				);
				for (_, pad) in srcpads.iter().by_ref() {
					element.remove_pad(pad).unwrap();
				}
			}
			_ => (),
		}

		ret
	}
}

struct DemuxerStatic {
	name: String,
	info: DemuxerInfo,
}

impl ImplTypeStatic<Element> for DemuxerStatic {
	fn get_name(&self) -> &str {
		self.name.as_str()
	}

	fn new(&self, element: &Element) -> Box<ElementImpl<Element>> {
		Demuxer::init(element, &self.info)
	}

	fn class_init(&self, klass: &mut ElementClass) {
		Demuxer::class_init(klass, &self.info);
	}
}

pub fn register(plugin: &gst::Plugin, info: DemuxerInfo) {
	let name = info.name.clone();
	let rank = info.rank;

	let demuxer_static = DemuxerStatic {
		name: name.clone(),
		info,
	};

	let typ = register_type(demuxer_static);
	gst::Element::register(plugin, &name, rank, typ);
}

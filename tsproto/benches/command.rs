#![feature(test)]
use std::borrow::Cow;
use std::marker::PhantomData;
use std::str;

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use tsproto_packets::commands::*;
use tsproto_packets::packets::*;

const SHORT_CMD: &[u8] = b"clientinitiv alpha=41Te9Ar7hMPx+A== omega=MEwDAgcAAgEgAiEAq2iCMfcijKDZ5tn2tuZcH+\\/GF+dmdxlXjDSFXLPGadACIHzUnbsPQ0FDt34Su4UXF46VFI0+4wjMDNszdoDYocu0 ip";
const LONG_CMD: &[u8] = b"channellist cid=2 cpid=0 channel_name=Trusted\\sChannel channel_topic channel_codec=0 channel_codec_quality=0 channel_maxclients=0 channel_maxfamilyclients=-1 channel_order=1 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=0 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic channel_icon_id=0 channel_flag_private=0|cid=4 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s1\\s\\p\\sSplamy\xc2\xb4s\\sBett channel_topic channel_codec=4 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=0 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=Neo\\sSeebi\\sEvangelion channel_icon_id=0 channel_flag_private=0|cid=6 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s2\\s\\p\\sThe\\sBook\\sof\\sHeavy\\sMetal channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=4 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=Not\\senought\\sChannels channel_icon_id=0 channel_flag_private=0|cid=30 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s3\\s\\p\\sSenpai\\sGef\xc3\xa4hrlich channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=6 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=The\\strashcan\\shas\\sthe\\strash channel_icon_id=0 channel_flag_private=0";

fn parse(b: &mut Bencher, cmd: &[u8]) {
	let cmd = str::from_utf8(cmd).unwrap();
	b.iter(|| parse_command(cmd).unwrap());
}

fn write(b: &mut Bencher, cmd: &[u8]) {
	let cmd = str::from_utf8(cmd).unwrap();
	let command = parse_command(cmd).unwrap();
	b.iter(|| {
		OutCommand::new(
			Direction::S2C,
			PacketType::Command,
			command.name,
			command.static_args.iter().map(|(k, v)| (*k, v.as_ref())),
			command
				.list_args
				.iter()
				.map(|i| i.iter().map(|(k, v)| (*k, v.as_ref()))),
		)
	});
}

// TODO Remove this code
trait InMsg {}

#[derive(Debug)]
pub struct InClientInitIv<'a> {
	list: Vec<ClientInitIvPart<'a>>,
}
impl InMsg for InClientInitIv<'_> {}

#[derive(Debug)]
pub struct ClientInitIvPart<'a> {
	pub alpha: Cow<'a, str>,
	pub omega: Cow<'a, str>,
	pub ip: Vec<Cow<'a, str>>,
	pub phantom: PhantomData<&'a ()>,
}

#[derive(Debug)]
pub struct InChannelList<'a> {
	list: Vec<ChannelListPart<'a>>,
}
impl InMsg for InChannelList<'_> {}

#[derive(Debug)]
pub struct ChannelListPart<'a> {
	pub channel_id: u64,
	pub parent_channel_id: u64,
	pub order: u64,
	pub name: Cow<'a, str>,
	pub total_clients: i32,
	pub needed_subscribe_power: i32,
	pub topic: Option<Cow<'a, str>>,
	pub is_default: Option<bool>,
	pub has_password: Option<bool>,
	pub is_permanent: Option<bool>,
	pub is_semi_permanent: Option<bool>,
	pub codec: Option<u8>,
	pub codec_quality: Option<u8>,
	pub needed_talk_power: Option<i32>,
	pub total_family_clients: Option<i32>,
	pub max_clients: Option<i32>,
	pub max_family_clients: Option<i32>,
	pub icon_id: Option<i64>,
	pub duration_empty: Option<u64>,
	pub phantom: PhantomData<&'a ()>,
}

#[derive(Debug)]
pub struct InClientInitIv2 {
	list: Vec<ClientInitIvPart2>,
}
impl InMsg for InClientInitIv2 {}

#[derive(Debug)]
pub struct ClientInitIvPart2 {
	pub alpha: String,
	pub omega: String,
	pub ip: Vec<String>,
}

#[derive(Debug)]
pub struct InChannelList2 {
	list: Vec<ChannelListPart2>,
}
impl InMsg for InChannelList2 {}

#[derive(Debug)]
pub struct ChannelListPart2 {
	pub channel_id: u64,
	pub parent_channel_id: u64,
	pub order: u64,
	pub name: String,
	pub total_clients: i32,
	pub needed_subscribe_power: i32,
	pub topic: Option<String>,
	pub is_default: Option<bool>,
	pub has_password: Option<bool>,
	pub is_permanent: Option<bool>,
	pub is_semi_permanent: Option<bool>,
	pub codec: Option<u8>,
	pub codec_quality: Option<u8>,
	pub needed_talk_power: Option<i32>,
	pub total_family_clients: Option<i32>,
	pub max_clients: Option<i32>,
	pub max_family_clients: Option<i32>,
	pub icon_id: Option<i64>,
	pub duration_empty: Option<u64>,
}

fn parse_message(b: &mut Bencher, cmd: &[u8]) {
	b.iter(|| {
		let cmd = str::from_utf8(cmd).unwrap();
		let data = parse_command(cmd).unwrap();
		let _ = std::hint::black_box::<Box<dyn InMsg>>(match data.name {
			"clientinitiv" => {
				if data.name != "clientinitiv" {
					panic!();
				}

				// List arguments
				let mut list = Vec::new();
				for ccmd in data.iter() {
					list.push(ClientInitIvPart {
						alpha: Cow::Borrowed(ccmd.0.get("alpha").unwrap()),
						omega: Cow::Borrowed(ccmd.0.get("omega").unwrap()),
						ip: {
							let val = ccmd.0.get("ip").unwrap();
							val.split(',')
								.filter_map(|val| {
									let val = val.trim();
									if val.is_empty() {
										None
									} else {
										Some(val)
									}
								}).map(|val| {
									let val = val.trim();
									Ok(val.into())
								}).collect::<Result<Vec<_>, ()>>().unwrap()
						},
						phantom: PhantomData,
					});
				}

				Box::new(InClientInitIv { list })
			}
			"channellist" => {
				if data.name != "channellist" {
					panic!();
				}

				// List arguments
				let mut list = Vec::new();
				for ccmd in data.iter() {
					list.push(ChannelListPart {
						channel_id: {
							let val = ccmd.0.get("cid")
								.unwrap();
									val.parse().unwrap()
						},
						parent_channel_id: {
							let val = ccmd.0.get("cpid")
								.unwrap();
									val.parse().unwrap()
						},
						order: {
							let val = ccmd.0.get("channel_order")
								.unwrap();
									val.parse().unwrap()
						},
						name: {
							let val = Cow::Borrowed(*ccmd.0.get("channel_name")
								.unwrap());
							val				},
						total_clients: 0,
						needed_subscribe_power: 0,
						topic: {
							if let Some(val) = ccmd.0.get("channel_topic") {
								Some({ Cow::Borrowed(val) })
							} else { None } },
						is_default: {
							if let Some(val) = ccmd.0.get("channel_flag_default") {
								Some({ 		match *val { "0" => false, "1" => true, _ => panic!() }
							})
							} else { None } },
						has_password: {
							if let Some(val) = ccmd.0.get("channel_flag_password") {
								Some({ 		match *val { "0" => false, "1" => true, _ => panic!() }
							})
							} else { None } },
						is_permanent: {
							if let Some(val) = ccmd.0.get("channel_flag_permanent") {
								Some({ 		match *val { "0" => false, "1" => true, _ => panic!() }
							})
							} else { None } },
						is_semi_permanent: {
							if let Some(val) = ccmd.0.get("channel_flag_semi_permanent") {
								Some({ 		match *val { "0" => false, "1" => true, _ => panic!() }
							})
							} else { None } },
						codec: {
							if let Some(val) = ccmd.0.get("channel_codec") {
								Some({ 		val.parse().unwrap()
							})
							} else { None } },
						codec_quality: {
							if let Some(val) = ccmd.0.get("channel_codec_quality") {
								Some({ 		val.parse().unwrap()
							})
							} else { None } },
						needed_talk_power: {
							if let Some(val) = ccmd.0.get("channel_needed_talk_power") {
								Some({ 		val.parse().unwrap()
							})
							} else { None } },
						total_family_clients: {
							if let Some(val) = ccmd.0.get("total_clients_family") {
								Some({ 		val.parse().unwrap()
							})
							} else { None } },
						max_clients: {
							if let Some(val) = ccmd.0.get("channel_maxclients") {
								Some({ 		val.parse().unwrap()
							})
							} else { None } },
						max_family_clients: {
							if let Some(val) = ccmd.0.get("channel_maxfamilyclients") {
								Some({ 		val.parse().unwrap()
							})
							} else { None } },
						icon_id: {
							if let Some(val) = ccmd.0.get("channel_icon_id") {
								Some({ 		val.parse().unwrap()
							})
							} else { None } },
						duration_empty: {
							if let Some(val) = ccmd.0.get("seconds_empty") {
								Some({ 		val.parse().unwrap()
							})
							} else { None } },
						phantom: PhantomData,
					});
				}

				Box::new(InChannelList { list })
			}
			_ => panic!(),
		});
	});
}

fn parse_message_new(b: &mut Bencher, cmd: &[u8]) {
	b.iter(|| {
		let (name, args) = CommandParser::new(cmd);
		let _ = std::hint::black_box::<Box<dyn InMsg>>(match name {
			b"clientinitiv" => {
				if name != b"clientinitiv" {
					panic!();
				}

				let mut alpha = None;
				let mut omega = None;
				let mut ip = None;

				let mut list = Vec::new();
				for item in args {
					match item {
						CommandItem::NextCommand => {
							// Build command
							list.push(ClientInitIvPart {
								alpha: alpha.clone().unwrap(),
								omega: omega.clone().unwrap(),
								ip: ip.clone().unwrap(),
								phantom: PhantomData,
							});
						}
						CommandItem::Argument(arg) => {
							match arg.name() {
								b"alpha" => {
									alpha = Some(arg.value().get_str().unwrap());
								}
								b"omega" => {
									omega = Some(arg.value().get_str().unwrap());
								}
								b"ip" => {
									let val = arg.value().get_str().unwrap();
									ip = Some(val.split(',')
										.filter_map(|val| {
											let val = val.trim();
											if val.is_empty() {
												None
											} else {
												Some(val)
											}
										}).map(|val| {
											let val = val.trim();
											// Could be optimized more
											Ok(val.to_string().into())
										}).collect::<Result<Vec<_>, ()>>().unwrap());
								}
								s => panic!("Unexpected {:?}", str::from_utf8(s)),
							}
						}
					}
				}
				// Build last command
				list.push(ClientInitIvPart {
					alpha: alpha.unwrap(),
					omega: omega.unwrap(),
					ip: ip.unwrap(),
					phantom: PhantomData,
				});

				Box::new(InClientInitIv { list })
			}
			b"channellist" => {
				if name != b"channellist" {
					panic!();
				}

				let mut channel_id = None;
				let mut parent_channel_id = None;
				let mut order = None;
				let mut name = None;
				let total_clients = Some(0);
				let needed_subscribe_power = Some(0);
				let mut topic = None;
				let mut is_default = None;
				let mut has_password = None;
				let mut is_permanent = None;
				let mut is_semi_permanent = None;
				let mut codec = None;
				let mut codec_quality = None;
				let mut needed_talk_power = None;
				let mut total_family_clients = None;
				let mut max_clients = None;
				let mut max_family_clients = None;
				let mut icon_id = None;
				let mut duration_empty = None;

				let mut list = Vec::new();
				for item in args {
					match item {
						CommandItem::NextCommand => {
							// Build command
							list.push(ChannelListPart {
								channel_id: channel_id.clone().unwrap(),
								parent_channel_id: parent_channel_id.clone().unwrap(),
								order: order.clone().unwrap(),
								name: name.clone().unwrap(),
								total_clients: total_clients.clone().unwrap(),
								needed_subscribe_power: needed_subscribe_power.clone().unwrap(),
								topic: topic.clone(),
								is_default: is_default.clone(),
								has_password: has_password.clone(),
								is_permanent: is_permanent.clone(),
								is_semi_permanent: is_semi_permanent.clone(),
								codec: codec.clone(),
								codec_quality: codec_quality.clone(),
								needed_talk_power: needed_talk_power.clone(),
								total_family_clients: total_family_clients.clone(),
								max_clients: max_clients.clone(),
								max_family_clients: max_family_clients.clone(),
								icon_id: icon_id.clone(),
								duration_empty: duration_empty.clone(),
								phantom: PhantomData,
							});
						}
						CommandItem::Argument(arg) => {
							match arg.name() {
								b"cid" => {
									channel_id = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"cpid" => {
									parent_channel_id = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_order" => {
									order = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_name" => {
									name = Some(arg.value().get_str().unwrap());
								}
								b"channel_topic" => {
									topic = Some(arg.value().get_str().unwrap());
								}
								b"channel_flag_default" => {
									is_default = Some(match arg.value().get_raw() {
										b"0" => false,
										b"1" => true,
										_ => panic!(),
									});
								}
								b"channel_flag_password" => {
									has_password = Some(match arg.value().get_raw() {
										b"0" => false,
										b"1" => true,
										_ => panic!(),
									});
								}
								b"channel_flag_permanent" => {
									is_permanent = Some(match arg.value().get_raw() {
										b"0" => false,
										b"1" => true,
										_ => panic!(),
									});
								}
								b"channel_flag_semi_permanent" => {
									is_semi_permanent = Some(match arg.value().get_raw() {
										b"0" => false,
										b"1" => true,
										_ => panic!(),
									});
								}
								b"channel_codec" => {
									codec = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_codec_quality" => {
									codec_quality = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_needed_talk_power" => {
									needed_talk_power = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"total_family_clients" => {
									total_family_clients = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_maxclients" => {
									max_clients = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_maxfamilyclients" => {
									max_family_clients = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_icon_id" => {
									icon_id = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"seconds_empty" => {
									duration_empty = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								s => {
									//panic!("Unexpected {:?}", str::from_utf8(s));
								}
							}
						}
					}
				}
				// Build last command
				list.push(ChannelListPart {
					channel_id: channel_id.unwrap(),
					parent_channel_id: parent_channel_id.unwrap(),
					order: order.unwrap(),
					name: name.unwrap(),
					total_clients: total_clients.unwrap(),
					needed_subscribe_power: needed_subscribe_power.unwrap(),
					topic,
					is_default,
					has_password,
					is_permanent,
					is_semi_permanent,
					codec,
					codec_quality,
					needed_talk_power,
					total_family_clients,
					max_clients,
					max_family_clients,
					icon_id,
					duration_empty,
					phantom: PhantomData,
				});

				Box::new(InChannelList { list })
			}
			_ => panic!(),
		});
	});
}

fn parse_message_new2(b: &mut Bencher, cmd: &[u8]) {
	b.iter(|| {
		let (name, args) = CommandParser::new(cmd);
		let _ = std::hint::black_box::<Box<dyn InMsg>>(match name {
			b"clientinitiv" => {
				if name != b"clientinitiv" {
					panic!();
				}

				let mut alpha = None;
				let mut omega = None;
				let mut ip = None;

				let mut list = Vec::new();
				for item in args {
					match item {
						CommandItem::NextCommand => {
							// Build command
							list.push(ClientInitIvPart2 {
								alpha: alpha.clone().unwrap(),
								omega: omega.clone().unwrap(),
								ip: ip.clone().unwrap(),
							});
						}
						CommandItem::Argument(arg) => {
							match arg.name() {
								b"alpha" => {
									alpha = Some(arg.value().get_str().unwrap().into_owned());
								}
								b"omega" => {
									omega = Some(arg.value().get_str().unwrap().into_owned());
								}
								b"ip" => {
									let val = arg.value().get_str().unwrap();
									ip = Some(val.split(',')
										.filter_map(|val| {
											let val = val.trim();
											if val.is_empty() {
												None
											} else {
												Some(val)
											}
										}).map(|val| {
											let val = val.trim();
											// Could be optimized more
											Ok(val.to_string().into())
										}).collect::<Result<Vec<_>, ()>>().unwrap());
								}
								s => panic!("Unexpected {:?}", str::from_utf8(s)),
							}
						}
					}
				}
				// Build last command
				list.push(ClientInitIvPart2 {
					alpha: alpha.unwrap(),
					omega: omega.unwrap(),
					ip: ip.unwrap(),
				});

				Box::new(InClientInitIv2 { list })
			}
			b"channellist" => {
				if name != b"channellist" {
					panic!();
				}

				let mut channel_id = None;
				let mut parent_channel_id = None;
				let mut order = None;
				let mut name = None;
				let total_clients = Some(0);
				let needed_subscribe_power = Some(0);
				let mut topic = None;
				let mut is_default = None;
				let mut has_password = None;
				let mut is_permanent = None;
				let mut is_semi_permanent = None;
				let mut codec = None;
				let mut codec_quality = None;
				let mut needed_talk_power = None;
				let mut total_family_clients = None;
				let mut max_clients = None;
				let mut max_family_clients = None;
				let mut icon_id = None;
				let mut duration_empty = None;

				let mut list = Vec::new();
				for item in args {
					match item {
						CommandItem::NextCommand => {
							// Build command
							list.push(ChannelListPart2 {
								channel_id: channel_id.clone().unwrap(),
								parent_channel_id: parent_channel_id.clone().unwrap(),
								order: order.clone().unwrap(),
								name: name.clone().unwrap(),
								total_clients: total_clients.clone().unwrap(),
								needed_subscribe_power: needed_subscribe_power.clone().unwrap(),
								topic: topic.clone(),
								is_default: is_default.clone(),
								has_password: has_password.clone(),
								is_permanent: is_permanent.clone(),
								is_semi_permanent: is_semi_permanent.clone(),
								codec: codec.clone(),
								codec_quality: codec_quality.clone(),
								needed_talk_power: needed_talk_power.clone(),
								total_family_clients: total_family_clients.clone(),
								max_clients: max_clients.clone(),
								max_family_clients: max_family_clients.clone(),
								icon_id: icon_id.clone(),
								duration_empty: duration_empty.clone(),
							});
						}
						CommandItem::Argument(arg) => {
							match arg.name() {
								b"cid" => {
									channel_id = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"cpid" => {
									parent_channel_id = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_order" => {
									order = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_name" => {
									name = Some(arg.value().get_str().unwrap().into_owned());
								}
								b"channel_topic" => {
									topic = Some(arg.value().get_str().unwrap().into_owned());
								}
								b"channel_flag_default" => {
									is_default = Some(match arg.value().get_raw() {
										b"0" => false,
										b"1" => true,
										_ => panic!(),
									});
								}
								b"channel_flag_password" => {
									has_password = Some(match arg.value().get_raw() {
										b"0" => false,
										b"1" => true,
										_ => panic!(),
									});
								}
								b"channel_flag_permanent" => {
									is_permanent = Some(match arg.value().get_raw() {
										b"0" => false,
										b"1" => true,
										_ => panic!(),
									});
								}
								b"channel_flag_semi_permanent" => {
									is_semi_permanent = Some(match arg.value().get_raw() {
										b"0" => false,
										b"1" => true,
										_ => panic!(),
									});
								}
								b"channel_codec" => {
									codec = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_codec_quality" => {
									codec_quality = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_needed_talk_power" => {
									needed_talk_power = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"total_family_clients" => {
									total_family_clients = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_maxclients" => {
									max_clients = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_maxfamilyclients" => {
									max_family_clients = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"channel_icon_id" => {
									icon_id = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								b"seconds_empty" => {
									duration_empty = Some(arg.value().get_str().unwrap().parse().unwrap());
								}
								s => {
									//panic!("Unexpected {:?}", str::from_utf8(s));
								}
							}
						}
					}
				}
				// Build last command
				list.push(ChannelListPart2 {
					channel_id: channel_id.unwrap(),
					parent_channel_id: parent_channel_id.unwrap(),
					order: order.unwrap(),
					name: name.unwrap(),
					total_clients: total_clients.unwrap(),
					needed_subscribe_power: needed_subscribe_power.unwrap(),
					topic,
					is_default,
					has_password,
					is_permanent,
					is_semi_permanent,
					codec,
					codec_quality,
					needed_talk_power,
					total_family_clients,
					max_clients,
					max_family_clients,
					icon_id,
					duration_empty,
				});

				Box::new(InChannelList2 { list })
			}
			_ => panic!(),
		});
	});
}

fn parse_short(c: &mut Criterion) {
	c.bench_function("parse short", |b| parse(b, SHORT_CMD));
}
fn parse_long(c: &mut Criterion) {
	c.bench_function("parse long", |b| parse(b, LONG_CMD));
}

fn write_short(c: &mut Criterion) {
	c.bench_function("write short", |b| write(b, SHORT_CMD));
}
fn write_long(c: &mut Criterion) {
	c.bench_function("write long", |b| write(b, LONG_CMD));
}

fn message_short(c: &mut Criterion) {
	c.bench_function("message short", |b| parse_message(b, SHORT_CMD));
}
fn message_long(c: &mut Criterion) {
	c.bench_function("message long", |b| parse_message(b, LONG_CMD));
}

fn message_short_new(c: &mut Criterion) {
	c.bench_function("message short new", |b| parse_message_new(b, SHORT_CMD));
}
fn message_long_new(c: &mut Criterion) {
	c.bench_function("message long new", |b| parse_message_new(b, LONG_CMD));
}

fn message_short_new2(c: &mut Criterion) {
	c.bench_function("message short new", |b| parse_message_new2(b, SHORT_CMD));
}
fn message_long_new2(c: &mut Criterion) {
	c.bench_function("message long new", |b| parse_message_new2(b, LONG_CMD));
}

criterion_group!(
	benches,
	parse_short,
	parse_long,
	write_short,
	write_long,
	message_short,
	message_long,
	message_short_new,
	message_long_new,
	message_short_new2,
	message_long_new2,
);
criterion_main!(benches);

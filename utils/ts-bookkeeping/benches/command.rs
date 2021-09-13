use std::iter;

use criterion::{criterion_group, criterion_main, Bencher, Criterion};
use once_cell::sync::Lazy;
use ts_bookkeeping::messages::s2c::{self, InMessage};
use tsproto_packets::packets::{Direction, Flags, OutPacket, PacketType};

const SHORT_CMD: &[u8] = b"notifyclientleftview cfid=1 ctid=0 clid=61";
const LONG_CMD: &[u8] = b"channellist cid=2 cpid=0 channel_name=Trusted\\sChannel channel_topic channel_codec=0 channel_codec_quality=0 channel_maxclients=0 channel_maxfamilyclients=-1 channel_order=1 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=0 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic channel_icon_id=0 channel_flag_private=0|cid=4 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s1\\s\\p\\sSplamy\xc2\xb4s\\sBett channel_topic channel_codec=4 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=0 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=Neo\\sSeebi\\sEvangelion channel_icon_id=0 channel_flag_private=0|cid=6 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s2\\s\\p\\sThe\\sBook\\sof\\sHeavy\\sMetal channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=4 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=Not\\senought\\sChannels channel_icon_id=0 channel_flag_private=0|cid=30 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s3\\s\\p\\sSenpai\\sGef\xc3\xa4hrlich channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=6 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=The\\strashcan\\shas\\sthe\\strash channel_icon_id=0 channel_flag_private=0";

static TRACING: Lazy<()> = Lazy::new(|| tracing_subscriber::fmt().with_test_writer().init());

fn parse(b: &mut Bencher, cmd: &[u8]) {
	Lazy::force(&TRACING);
	let header = OutPacket::new_with_dir(Direction::S2C, Flags::empty(), PacketType::Command);

	b.iter(|| InMessage::new(&header.header(), cmd).unwrap());
}

fn write(b: &mut Bencher, cmd: &[u8]) {
	Lazy::force(&TRACING);
	let header = OutPacket::new_with_dir(Direction::S2C, Flags::empty(), PacketType::Command);
	let msg = InMessage::new(&header.header(), cmd).unwrap();
	match msg {
		InMessage::ClientLeftView(msg) => {
			let out_part = msg.iter().next().unwrap().as_out();
			b.iter(|| s2c::OutClientLeftViewMessage::new(&mut iter::once(out_part.clone())));
		}
		InMessage::ChannelList(msg) => {
			let out_part = msg.iter().next().unwrap().as_out();
			b.iter(|| s2c::OutChannelListMessage::new(&mut iter::once(out_part.clone())));
		}
		_ => unreachable!("This command type is not supported in this test"),
	}
}

fn parse_short(c: &mut Criterion) { c.bench_function("parse short", |b| parse(b, SHORT_CMD)); }
fn parse_long(c: &mut Criterion) { c.bench_function("parse long", |b| parse(b, LONG_CMD)); }

fn write_short(c: &mut Criterion) { c.bench_function("write short", |b| write(b, SHORT_CMD)); }
fn write_long(c: &mut Criterion) { c.bench_function("write long", |b| write(b, LONG_CMD)); }

criterion_group!(benches, parse_short, parse_long, write_short, write_long);
criterion_main!(benches);

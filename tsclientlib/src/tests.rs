use std::sync::Mutex;

use slog::{o, Drain, Logger};
use ts_bookkeeping::messages::s2c::InMessage;
use tsproto_packets::packets::{Direction, Flags, OutPacket, PacketType};

pub(crate) fn get_logger() -> Logger {
	let decorator = slog_term::PlainDecorator::new(slog_term::TestStdoutWriter);
	let drain = Mutex::new(slog_term::FullFormat::new(decorator).build()).fuse();

	slog::Logger::root(drain, o!())
}

fn parse_msg(msg: &str) -> InMessage {
	let header = OutPacket::new_with_dir(Direction::S2C, Flags::empty(), PacketType::Command);

	let logger = get_logger();

	InMessage::new(&logger, &header.header(), msg.as_bytes()).unwrap()
}

fn test_iconid(input: &str, expected: u32) {
	let msg = parse_msg(&format!(
		r#"initserver virtualserver_welcomemessage virtualserver_platform virtualserver_version virtualserver_maxclients=0 virtualserver_created=0 virtualserver_hostmessage virtualserver_hostmessage_mode=0 virtualserver_id=0 virtualserver_ip virtualserver_ask_for_privilegekey=0 acn aclid=0 pv=0 client_talk_power=0 client_needed_serverquery_view_power=0 virtualserver_name virtualserver_codec_encryption_mode=0 virtualserver_default_server_group=0 virtualserver_default_channel_group=0 virtualserver_hostbanner_url virtualserver_hostbanner_gfx_url virtualserver_hostbanner_gfx_interval=0 virtualserver_priority_speaker_dimm_modificator=0 virtualserver_hostbutton_tooltip virtualserver_hostbutton_url virtualserver_hostbutton_gfx_url virtualserver_name_phonetic virtualserver_hostbanner_mode=0 virtualserver_channel_temp_delete_delay_default=0 virtualserver_icon_id={}"#,
		input
	));
	if let InMessage::InitServer(list) = msg {
		let cmd = list.iter().next().unwrap();
		assert_eq!(cmd.icon_id, ts_bookkeeping::IconHash(expected));
	} else {
		panic!("Failed to parse as initserver");
	}
}

#[test]
fn normal_iconid() { test_iconid("96136942", 96136942); }

#[test]
fn negative_iconid() { test_iconid("-96136942", 4198830354); }

#[test]
fn big_iconid() { test_iconid("18446744073225738240", 3811153920); }

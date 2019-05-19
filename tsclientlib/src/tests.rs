use tsproto_packets::packets::{Direction, InCommand, PacketType};
use ts_bookkeeping::messages::s2c::{InMessage, InMessages};

fn parse_msg(msg: &str) -> InMessage {
	let cmd = InCommand::new(
		msg.as_bytes().to_vec(),
		PacketType::Command,
		false,
		Direction::S2C,
	)
	.unwrap();

	InMessage::new(cmd).unwrap()
}

fn test_iconid(input: &str, expected: u32) {
	let msg = parse_msg(&format!(r#"initserver virtualserver_welcomemessage virtualserver_platform virtualserver_version virtualserver_maxclients=0 virtualserver_created=0 virtualserver_hostmessage virtualserver_hostmessage_mode=0 virtualserver_id=0 virtualserver_ip virtualserver_ask_for_privilegekey=0 acn aclid=0 pv=0 client_talk_power=0 client_needed_serverquery_view_power=0 virtualserver_name virtualserver_codec_encryption_mode=0 virtualserver_default_server_group=0 virtualserver_default_channel_group=0 virtualserver_hostbanner_url virtualserver_hostbanner_gfx_url virtualserver_hostbanner_gfx_interval=0 virtualserver_priority_speaker_dimm_modificator=0 virtualserver_hostbutton_tooltip virtualserver_hostbutton_url virtualserver_hostbutton_gfx_url virtualserver_name_phonetic virtualserver_hostbanner_mode=0 virtualserver_channel_temp_delete_delay_default=0 virtualserver_icon_id={}"#, input));
	if let InMessages::InitServer(list) = msg.msg() {
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

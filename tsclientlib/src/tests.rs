use once_cell::sync::Lazy;
use ts_bookkeeping::messages::s2c::InMessage;
use tsproto_packets::packets::{Direction, Flags, OutPacket, PacketType};

static TRACING: Lazy<()> = Lazy::new(|| tracing_subscriber::fmt().with_test_writer().init());

pub(crate) fn create_logger() { Lazy::force(&TRACING); }

fn parse_msg(msg: &str) -> InMessage {
	let header = OutPacket::new_with_dir(Direction::S2C, Flags::empty(), PacketType::Command);

	InMessage::new(&header.header(), msg.as_bytes()).unwrap()
}

fn test_iconid(input: &str, expected: u32) {
	create_logger();

	let msg = parse_msg(&format!(
		r#"initserver virtualserver_name=TeamSpeak\s]I[\sServer virtualserver_welcomemessage=Welcome\sto\sTeamSpeak,\scheck\s[URL]www.teamspeak.com[\/URL]\sfor\slatest\sinformation virtualserver_platform=Linux virtualserver_version=3.11.0\s[Build:\s1578903157] virtualserver_maxclients=32 virtualserver_created=1571572631 virtualserver_codec_encryption_mode=2 virtualserver_hostmessage virtualserver_hostmessage_mode=0 virtualserver_default_server_group=8 virtualserver_default_channel_group=8 virtualserver_hostbanner_url virtualserver_hostbanner_gfx_url virtualserver_hostbanner_gfx_interval=0 virtualserver_priority_speaker_dimm_modificator=-18.0000 virtualserver_id=1 virtualserver_hostbutton_tooltip virtualserver_hostbutton_url virtualserver_hostbutton_gfx_url virtualserver_name_phonetic virtualserver_ip=0.0.0.0,\s:: virtualserver_ask_for_privilegekey=0 virtualserver_hostbanner_mode=0 virtualserver_channel_temp_delete_delay_default=0 virtualserver_nickname client_nickname=TeamSpeakUser client_version=3.?.?\s[Build:\s5680278000] client_platform=Windows client_input_muted=0 client_output_muted=0 client_outputonly_muted=0 client_input_hardware=1 client_output_hardware=1 client_default_channel client_default_channel_password client_server_password client_meta_data client_version_sign=DX5NIYLvfJEUjuIbCidnoeozxIDRRkpq3I9vVMBmE9L2qnekOoBzSenkzsg2lC9CMv8K5hkEzhr2TYUYSwUXCg== client_security_hash client_key_offset=354 client_away=0 client_away_message client_nickname_phonetic client_default_token client_badges client_myteamspeak_id client_integrations client_active_integrations_info client_myteamspeak_avatar client_signed_badges acn=TeamSpeakUser aclid=2 pv=7 client_talk_power=75 client_needed_serverquery_view_power=75 virtualserver_icon_id={}"#,
		input
	));
	if let InMessage::InitServer(list) = msg {
		let cmd = list.iter().next().unwrap();
		assert_eq!(cmd.icon, ts_bookkeeping::IconId(expected));
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

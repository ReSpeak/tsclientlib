use tsproto::packets::{Direction, InCommand, PacketType};
use tsproto_commands::messages::s2c::{InMessage, InMessages};

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
	let msg = parse_msg(&format!(r#"channellist cid=1 cpid=0 channel_name=Name channel_topic channel_codec=4 channel_codec_quality=10 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=0 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=0 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic channel_flag_private=0 channel_icon_id={}"#, input));
	if let InMessages::ChannelList(list) = msg.msg() {
		let cmd = list.iter().next().unwrap();
		assert_eq!(cmd.icon_id, tsproto_commands::IconHash(expected));
	} else {
		panic!("Failed to parse as channellist");
	}
}

#[test]
fn normal_iconid() { test_iconid("96136942", 96136942); }

#[test]
fn negative_iconid() { test_iconid("-96136942", 4198830354); }

#[test]
fn big_iconid() { test_iconid("18446744073225738240", 3811153920); }

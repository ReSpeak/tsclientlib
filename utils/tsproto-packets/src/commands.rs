use std::borrow::Cow;
use std::str::{self, FromStr};

use crate::{Error, Result};

/// Parses arguments of a command.
#[derive(Clone, Debug)]
pub struct CommandParser<'a> {
	data: &'a [u8],
	index: usize,
}

#[derive(Clone, Debug)]
pub enum CommandItem<'a> {
	Argument(CommandArgument<'a>),
	/// Pipe symbol marking the start of the next command.
	NextCommand,
}

#[derive(Clone, Debug)]
pub struct CommandArgument<'a> {
	name: &'a [u8],
	value: CommandArgumentValue<'a>,
}

#[derive(Clone, Debug)]
pub struct CommandArgumentValue<'a> {
	raw: &'a [u8],
	/// The number of escape sequences in this value.
	escapes: usize,
}

impl<'a> CommandParser<'a> {
	/// Returns the name and arguments of the given command.
	#[inline]
	pub fn new(data: &'a [u8]) -> (&'a [u8], Self) {
		let mut name_end = 0;
		while name_end < data.len() {
			if !data[name_end].is_ascii_alphanumeric() {
				if data[name_end] == b' ' {
					break;
				}
				// Not a command name
				name_end = 0;
				break;
			}
			name_end += 1;
		}

		(&data[..name_end], Self { data, index: name_end })
	}

	fn cur(&self) -> u8 { self.data[self.index] }
	fn cur_in(&self, cs: &[u8]) -> bool { cs.contains(&self.cur()) }
	/// Advance
	fn adv(&mut self) { self.index += 1; }
	fn at_end(&self) -> bool { self.index >= self.data.len() }

	fn skip_space(&mut self) {
		while !self.at_end() && self.cur_in(b"\x0b\x0c\t\r\n ") {
			self.adv();
		}
	}
}

impl<'a> Iterator for CommandParser<'a> {
	type Item = CommandItem<'a>;
	fn next(&mut self) -> Option<Self::Item> {
		self.skip_space();
		if self.at_end() {
			return None;
		}
		if self.cur() == b'|' {
			self.adv();
			return Some(CommandItem::NextCommand);
		}

		let name_start = self.index;
		while !self.at_end() && !self.cur_in(b" =|") {
			self.adv();
		}
		let name_end = self.index;
		if self.at_end() || self.cur() != b'=' {
			return Some(CommandItem::Argument(CommandArgument {
				name: &self.data[name_start..name_end],
				value: CommandArgumentValue { raw: &[], escapes: 0 },
			}));
		}

		self.adv();
		let value_start = self.index;
		let mut escapes = 0;
		while !self.at_end() {
			if self.cur_in(b"\x0b\x0c\t\r\n| ") {
				break;
			}
			if self.cur() == b'\\' {
				escapes += 1;
				self.adv();
				if self.at_end() || self.cur_in(b"\x0b\x0c\t\r\n| ") {
					break;
				}
			}
			self.adv();
		}
		let value_end = self.index;
		Some(CommandItem::Argument(CommandArgument {
			name: &self.data[name_start..name_end],
			value: CommandArgumentValue { raw: &self.data[value_start..value_end], escapes },
		}))
	}
}

impl<'a> CommandArgument<'a> {
	#[inline]
	pub fn name(&self) -> &'a [u8] { self.name }
	#[inline]
	pub fn value(&self) -> &CommandArgumentValue<'a> { &self.value }
}

impl<'a> CommandArgumentValue<'a> {
	fn unescape(&self) -> Vec<u8> {
		let mut res = Vec::with_capacity(self.raw.len() - self.escapes);
		let mut i = 0;
		while i < self.raw.len() {
			if self.raw[i] == b'\\' {
				i += 1;
				if i == self.raw.len() {
					return res;
				}
				res.push(match self.raw[i] {
					b'v' => b'\x0b',
					b'f' => b'\x0c',
					b't' => b'\t',
					b'r' => b'\r',
					b'n' => b'\n',
					b'p' => b'|',
					b's' => b' ',
					c => c,
				});
			} else {
				res.push(self.raw[i]);
			}
			i += 1;
		}
		res
	}

	#[inline]
	pub fn get_raw(&self) -> &'a [u8] { self.raw }
	#[inline]
	pub fn get(&self) -> Cow<'a, [u8]> {
		if self.escapes == 0 { Cow::Borrowed(self.raw) } else { Cow::Owned(self.unescape()) }
	}

	#[inline]
	pub fn get_str(&self) -> Result<Cow<'a, str>> {
		if self.escapes == 0 {
			Ok(Cow::Borrowed(str::from_utf8(self.raw)?))
		} else {
			Ok(Cow::Owned(String::from_utf8(self.unescape())?))
		}
	}

	#[inline]
	pub fn get_parse<E, T: FromStr>(&self) -> std::result::Result<T, E>
	where
		E: From<<T as FromStr>::Err>,
		E: From<Error>,
	{
		Ok(self.get_str()?.as_ref().parse()?)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::packets::{Direction, Flags, OutCommand, PacketType};
	use std::str;

	/// Parse and write again.
	fn test_loop_with_result(data: &[u8], result: &[u8]) {
		let (name, parser) = CommandParser::new(data);
		let name = str::from_utf8(name).unwrap();

		let mut out_command =
			OutCommand::new(Direction::S2C, Flags::empty(), PacketType::Command, name);

		println!("\nParsing {}", str::from_utf8(data).unwrap());
		for item in parser {
			println!("Item: {:?}", item);
			match item {
				CommandItem::NextCommand => out_command.start_new_part(),
				CommandItem::Argument(arg) => {
					out_command.write_arg(
						str::from_utf8(arg.name()).unwrap(),
						&arg.value().get_str().unwrap(),
					);
				}
			}
		}

		let packet = out_command.into_packet();
		let in_str = str::from_utf8(result).unwrap();
		let out_str = str::from_utf8(packet.content()).unwrap();
		assert_eq!(in_str, out_str);
	}

	/// Parse and write again.
	fn test_loop(data: &[u8]) { test_loop_with_result(data, data); }

	const TEST_COMMANDS: &[&str] = &[
		"cmd a=1 b=2 c=3",
		"cmd a=\\s\\\\ b=\\p c=abc\\tdef",
		"cmd a=1 c=3 b=2|b=4|b=5",
		"initivexpand2 l=AQCVXTlKF+UQc0yga99dOQ9FJCwLaJqtDb1G7xYPMvHFMwIKVfKADF6zAAcAAAAgQW5vbnltb3VzAAAKQo71lhtEMbqAmtuMLlY8Snr0k2Wmymv4hnHNU6tjQCALKHewCykgcA== beta=\\/8kL8lcAYyMJovVOP6MIUC1oZASyuL\\/Y\\/qjVG06R4byuucl9oPAvR7eqZI7z8jGm9jkGmtJ6 omega=MEsDAgcAAgEgAiBxu2eCLQf8zLnuJJ6FtbVjfaOa1210xFgedoXuGzDbTgIgcGk35eqFavKxS4dROi5uKNSNsmzIL4+fyh5Z\\/+FWGxU= ot=1 proof=MEUCIQDRCP4J9e+8IxMJfCLWWI1oIbNPGcChl+3Jr2vIuyDxzAIgOrzRAFPOuJZF4CBw\\/xgbzEsgKMtEtgNobF6WXVNhfUw= tvd time=1544221457",

		"clientinitiv alpha=41Te9Ar7hMPx+A== omega=MEwDAgcAAgEgAiEAq2iCMfcijKDZ5tn2tuZcH+\\/GF+dmdxlXjDSFXLPGadACIHzUnbsPQ0FDt34Su4UXF46VFI0+4wjMDNszdoDYocu0 ip",

		// Well, that's more corrupted packet, but the parser should be robust
		"initserver virtualserver_name=Server\\sder\\sVerplanten \
		 virtualserver_welcomemessage=This\\sis\\sSplamys\\sWorld \
		 virtualserver_platform=Linux \
		 virtualserver_version=3.0.13.8\\s[Build:\\s1500452811] \
		 virtualserver_maxclients=32 virtualserver_created=0 \
		 virtualserver_nodec_encryption_mode=1 \
		 virtualserver_hostmessage=Lé\\sServer\\sde\\sSplamy \
		 virtualserver_name=Server_mode=0 virtualserver_default_server \
		 group=8 virtualserver_default_channel_group=8 \
		 virtualserver_hostbanner_url virtualserver_hostmessagegfx_url \
		 virtualserver_hostmessagegfx_interval=2000 \
		 virtualserver_priority_speaker_dimm_modificat",

		"channellist cid=2 cpid=0 channel_name=Trusted\\sChannel \
		 channel_topic channel_codec=0 channel_codec_quality=0 \
		 channel_maxclients=0 channel_maxfamilyclients=-1 channel_order=1 \
		 channel_flag_permanent=1 channel_flag_semi_permanent=0 \
		 channel_flag_default=0 channel_flag_password=0 \
		 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 \
		 channel_delete_delay=0 channel_flag_maxclients_unlimited=0 \
		 channel_flag_maxfamilyclients_unlimited=0 \
		 channel_flag_maxfamilyclients_inherited=1 \
		 channel_needed_talk_power=0 channel_forced_silence=0 \
		 channel_name_phonetic channel_icon_id=0 \
		 channel_flag_private=0|cid=4 cpid=2 \
		 channel_name=Ding\\s•\\s1\\s\\p\\sSplamy´s\\sBett channel_topic \
		 channel_codec=4 channel_codec_quality=7 channel_maxclients=-1 \
		 channel_maxfamilyclients=-1 channel_order=0 \
		 channel_flag_permanent=1 channel_flag_semi_permanent=0 \
		 channel_flag_default=0 channel_flag_password=0 \
		 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 \
		 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 \
		 channel_flag_maxfamilyclients_unlimited=0 \
		 channel_flag_maxfamilyclients_inherited=1 \
		 channel_needed_talk_power=0 channel_forced_silence=0 \
		 channel_name_phonetic=Neo\\sSeebi\\sEvangelion channel_icon_id=0 \
		 channel_flag_private=0", //|cid=6 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s2\\s\\p\\sThe\\sBook\\sof\\sHeavy\\sMetal channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=4 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=Not\\senought\\sChannels channel_icon_id=0 channel_flag_private=0|cid=30 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s3\\s\\p\\sSenpai\\sGef\xc3\xa4hrlich channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=6 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=The\\strashcan\\shas\\sthe\\strash channel_icon_id=0 channel_flag_private=0",

		"notifychannelsubscribed cid=2|cid=4 es=3867|cid=5 es=18694|cid=6 es=18694|cid=7 es=18694|cid=11 es=18694|cid=13 es=18694|cid=14 es=18694|cid=16 es=18694|cid=22 es=18694|cid=23 es=18694|cid=24 es=18694|cid=25 es=18694|cid=30 es=18694|cid=163 es=18694",

		"notifypermissionlist group_id_end=0|group_id_end=7|group_id_end=13|group_id_end=18|group_id_end=21|group_id_end=21|group_id_end=33|group_id_end=47|group_id_end=77|group_id_end=82|group_id_end=83|group_id_end=106|group_id_end=126|group_id_end=132|group_id_end=143|group_id_end=151|group_id_end=160|group_id_end=162|group_id_end=170|group_id_end=172|group_id_end=190|group_id_end=197|group_id_end=215|group_id_end=227|group_id_end=232|group_id_end=248|permname=b_serverinstance_help_view permdesc=Retrieve\\sinformation\\sabout\\sServerQuery\\scommands|permname=b_serverinstance_version_view permdesc=Retrieve\\sglobal\\sserver\\sversion\\s(including\\splatform\\sand\\sbuild\\snumber)|permname=b_serverinstance_info_view permdesc=Retrieve\\sglobal\\sserver\\sinformation|permname=b_serverinstance_virtualserver_list permdesc=List\\svirtual\\sservers\\sstored\\sin\\sthe\\sdatabase",

		// Server query
		"cmd=1 cid=2",
		"channellistfinished",
		// With newlines
		"sendtextmessage text=\\nmess\\nage\\n return_code=11",
	];

	#[test]
	fn loop_test() {
		for cmd in TEST_COMMANDS {
			test_loop(cmd.as_bytes());
		}
	}

	#[test]
	fn optional_arg() {
		test_loop(b"cmd a");
		test_loop(b"cmd a b=1");

		test_loop_with_result(b"cmd a=", b"cmd a");
		test_loop_with_result(b"cmd a= b=1", b"cmd a b=1");
	}

	#[test]
	fn no_slash_escape() {
		let in_cmd = "clientinitiv alpha=giGMvmfHzbY3ig== omega=MEsDAgcAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50= ot=1 ip";
		let out_cmd = "clientinitiv alpha=giGMvmfHzbY3ig== omega=MEsDAgcAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC\\/50= ot=1 ip";

		test_loop_with_result(in_cmd.as_bytes(), out_cmd.as_bytes());
	}
}

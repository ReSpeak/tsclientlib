use std::borrow::Cow;
use std::collections::HashMap;
use std::mem;
use std::str::{self, FromStr};

use nom::branch::alt;
use nom::bytes::complete::{is_not, tag};
use nom::character::complete::{alphanumeric1, multispace0, multispace1};
use nom::combinator::{map, opt};
use nom::multi::{many0, many1};
use nom::IResult;

use crate::{Error, Result};

fn command_arg(i: &str) -> IResult<&str, (&str, Cow<str>)> {
	let (i, _) = multispace0(i)?;
	let (i, name) = is_not("\u{b}\u{c}\\\t\r\n| /=")(i)?;
	let (i, value) = opt(|i| {
		let (i, _) = tag("=")(i)?;
		let (i, prefix) = opt(is_not("\u{b}\u{c}\\\t\r\n| "))(i)?;
		let (i, rest) = many0(alt((
			map(tag("\\v"), |_| "\x0b"), // Vertical tab
			map(tag("\\f"), |_| "\x0c"), // Form feed
			map(tag("\\\\"), |_| "\\"),
			map(tag("\\t"), |_| "\t"),
			map(tag("\\r"), |_| "\r"),
			map(tag("\\n"), |_| "\n"),
			map(tag("\\p"), |_| "|"),
			map(tag("\\s"), |_| " "),
			map(tag("\\/"), |_| "/"),
			is_not("\u{b}\u{c}\\\t\r\n| "),
		)))(i)?;

		let res = if rest.is_empty() {
			Cow::Borrowed(prefix.unwrap_or(""))
		} else {
			Cow::Owned(format!("{}{}", prefix.unwrap_or(""), rest.concat()))
		};

		Ok((i, res))
	})(i)?;
	let value = value.unwrap_or(Cow::Borrowed(""));

	Ok((i, (name, value)))
}

fn inner_parse_command<'a>(i: &'a str) -> IResult<&'a str, CommandData> {
	let (i, name) = alt((
		|i: &'a str| {
			let (i, res) = alphanumeric1(i)?;
			let i = if i.is_empty() { i } else { multispace1(i)?.0 };
			Ok((i, res))
		},
		tag(""),
	))(i)?;

	let (i, static_args) = many0(command_arg)(i)?;
	let (i, list_args) = many0(|i| {
		let (i, _) = multispace0(i)?;
		let (i, _) = tag("|")(i)?;
		let (i, args) = many1(command_arg)(i)?;
		Ok((i, args))
	})(i)?;

	let (i, _) = multispace0(i)?;

	Ok((i, CommandData { name, static_args, list_args }))
}

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

#[derive(Clone, Debug)]
pub struct CommandData<'a> {
	/// The name is empty for serverquery commands
	pub name: &'a str,
	pub static_args: Vec<(&'a str, Cow<'a, str>)>,
	pub list_args: Vec<Vec<(&'a str, Cow<'a, str>)>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CanonicalCommand<'a>(pub HashMap<&'a str, &'a str>);
impl<'a> CanonicalCommand<'a> {
	#[inline]
	pub fn has(&self, arg: &str) -> bool { self.0.contains_key(arg) }
	#[inline]
	pub fn get(&self, arg: &str) -> Option<&str> { self.0.get(arg).map(|s| *s) }

	pub fn get_parse<F: FromStr>(
		&self, arg: &str,
	) -> std::result::Result<F, Option<<F as FromStr>::Err>> {
		if let Some(s) = self.0.get(arg) {
			s.parse::<F>().map_err(Some)
		} else {
			Err(None)
		}
	}
}

pub fn parse_command(s: &str) -> Result<CommandData> {
	match inner_parse_command(s) {
		Ok((rest, mut cmd)) => {
			// Error if rest contains something
			if !rest.is_empty() {
				return Err(crate::Error::ParseCommand(format!(
					"Command was not parsed completely {:?}",
					rest
				)));
			}

			// Some of the static args are variable so move the to the right
			// category.
			if !cmd.list_args.is_empty() {
				let mut la = Vec::new();
				for &(ref arg, _) in &cmd.list_args[0] {
					if let Some(i) =
						cmd.static_args.iter().position(|&(ref k, _)| k == arg)
					{
						la.push(cmd.static_args.remove(i));
					} else {
						// Not a valid command list, but ignore it
					}
				}
				cmd.list_args.insert(0, la);
			}
			Ok(cmd)
		}
		Err(e) => Err(crate::Error::ParseCommand(format!("{:?}", e))),
	}
}

pub struct CommandDataIterator<'a> {
	cmd: &'a CommandData<'a>,
	pub statics: HashMap<&'a str, &'a str>,
	i: usize,
}

impl<'a> CommandParser<'a> {
	/// Returns the name and arguments of the given command.
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
			if self.cur_in(b"\x0b\x0c\t\r\n|/ ") {
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
			value: CommandArgumentValue {
				raw: &self.data[value_start..value_end],
				escapes,
			},
		}))
	}
}

impl<'a> CommandArgument<'a> {
	pub fn name(&self) -> &'a [u8] { self.name }
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

	pub fn get_raw(&self) -> &'a [u8] { self.raw }
	pub fn get(&self) -> Cow<'a, [u8]> {
		if self.escapes == 0 {
			Cow::Borrowed(self.raw)
		} else {
			Cow::Owned(self.unescape())
		}
	}

	pub fn get_str(&self) -> Result<Cow<'a, str>> {
		if self.escapes == 0 {
			Ok(Cow::Borrowed(str::from_utf8(self.raw)?))
		} else {
			Ok(Cow::Owned(String::from_utf8(self.unescape())?))
		}
	}

	pub fn get_parse<E, T: FromStr>(&self) -> std::result::Result<T, E>
	where
		E: From<<T as FromStr>::Err>,
		E: From<Error>,
	{
		Ok(self.get_str()?.as_ref().parse()?)
	}
}

impl<'a> Iterator for CommandDataIterator<'a> {
	type Item = CanonicalCommand<'a>;
	fn next(&mut self) -> Option<Self::Item> {
		let i = self.i;
		self.i += 1;
		if self.cmd.list_args.is_empty() {
			if i == 0 {
				Some(CanonicalCommand(mem::replace(
					&mut self.statics,
					HashMap::new(),
				)))
			} else {
				None
			}
		} else if i < self.cmd.list_args.len() {
			let l = &self.cmd.list_args[i];
			let mut v = self.statics.clone();
			v.extend(l.iter().map(|(k, v)| (*k, v.as_ref())));
			Some(CanonicalCommand(v))
		} else {
			None
		}
	}
}

impl<'a> CommandData<'a> {
	#[inline]
	pub fn static_arg(&self, k: &str) -> Option<&str> {
		self.static_args
			.iter()
			.find_map(|(k2, v)| if *k2 == k { Some(v.as_ref()) } else { None })
	}

	#[inline]
	pub fn iter(&self) -> CommandDataIterator { self.into_iter() }
}

impl<'a> IntoIterator for &'a CommandData<'a> {
	type Item = crate::commands::CanonicalCommand<'a>;
	type IntoIter = CommandDataIterator<'a>;
	fn into_iter(self) -> Self::IntoIter {
		let statics =
			self.static_args.iter().map(|(a, b)| (*a, b.as_ref())).collect();
		CommandDataIterator { cmd: self, statics, i: 0 }
	}
}

#[cfg(test)]
mod tests {
	use super::{parse_command, CommandData};
	use crate::packets::OutCommand;
	use std::str;

	/// Parse and write again.
	fn test_loop(s: &str) -> CommandData {
		let command = parse_command(s).unwrap();
		println!("Parsed command: {:?}", command);
		let mut written = Vec::new();
		OutCommand::new_into(
			command.name,
			command.static_args.iter().map(|(k, v)| (*k, v.as_ref())),
			command
				.list_args
				.iter()
				.map(|i| i.iter().map(|(k, v)| (*k, v.as_ref()))),
			&mut written,
		);

		assert_eq!(s, str::from_utf8(&written).unwrap());
		command
	}

	#[test]
	fn simple() {
		let cmd = test_loop("cmd a=1 b=2 c=3");
		assert_eq!(cmd.name, "cmd");
		assert_eq!(cmd.static_args, vec![
			("a", "1".into()),
			("b", "2".into()),
			("c", "3".into()),
		]);
		assert!(cmd.list_args.is_empty());
	}

	#[test]
	fn escape() {
		let cmd = test_loop("cmd a=\\s\\\\ b=\\p c=abc\\tdef");
		assert_eq!(cmd.name, "cmd");
		assert_eq!(cmd.static_args, vec![
			("a", " \\".into()),
			("b", "|".into()),
			("c", "abc\tdef".into()),
		]);
		assert!(cmd.list_args.is_empty());
	}

	#[test]
	fn array() {
		let cmd = test_loop("cmd a=1 c=3 b=2|b=4|b=5");
		assert_eq!(cmd.name, "cmd");
		assert_eq!(
			cmd.static_args,
			vec![("a", "1".into()), ("c", "3".into()),]
		);
		assert_eq!(cmd.list_args, vec![
			vec![("b", "2".into())],
			vec![("b", "4".into())],
			vec![("b", "5".into())],
		]);
	}

	#[test]
	fn optional_arg() {
		let cmd = test_loop("cmd a");
		assert_eq!(cmd.name, "cmd");
		assert_eq!(cmd.static_args, vec![("a", "".into())]);
		assert!(cmd.list_args.is_empty());

		let cmd = test_loop("cmd a b=1");
		assert_eq!(cmd.name, "cmd");
		assert_eq!(cmd.static_args, vec![("a", "".into()), ("b", "1".into())]);
		assert!(cmd.list_args.is_empty());

		let cmd = parse_command("cmd a=").unwrap();
		assert_eq!(cmd.name, "cmd");
		assert_eq!(cmd.static_args, vec![("a", "".into())]);
		assert!(cmd.list_args.is_empty());

		let cmd = parse_command("cmd a= b=1").unwrap();
		assert_eq!(cmd.name, "cmd");
		assert_eq!(cmd.static_args, vec![("a", "".into()), ("b", "1".into())]);
		assert!(cmd.list_args.is_empty());
	}

	#[test]
	fn initivexpand2() {
		let cmd = test_loop("initivexpand2 l=AQCVXTlKF+UQc0yga99dOQ9FJCwLaJqtDb1G7xYPMvHFMwIKVfKADF6zAAcAAAAgQW5vbnltb3VzAAAKQo71lhtEMbqAmtuMLlY8Snr0k2Wmymv4hnHNU6tjQCALKHewCykgcA== beta=\\/8kL8lcAYyMJovVOP6MIUC1oZASyuL\\/Y\\/qjVG06R4byuucl9oPAvR7eqZI7z8jGm9jkGmtJ6 omega=MEsDAgcAAgEgAiBxu2eCLQf8zLnuJJ6FtbVjfaOa1210xFgedoXuGzDbTgIgcGk35eqFavKxS4dROi5uKNSNsmzIL4+fyh5Z\\/+FWGxU= ot=1 proof=MEUCIQDRCP4J9e+8IxMJfCLWWI1oIbNPGcChl+3Jr2vIuyDxzAIgOrzRAFPOuJZF4CBw\\/xgbzEsgKMtEtgNobF6WXVNhfUw= tvd time=1544221457");
		assert_eq!(cmd.name, "initivexpand2");
	}

	#[test]
	fn clientinitiv() {
		let cmd = test_loop(
			"clientinitiv alpha=41Te9Ar7hMPx+A== \
			 omega=MEwDAgcAAgEgAiEAq2iCMfcijKDZ5tn2tuZcH+\\/\
			 GF+dmdxlXjDSFXLPGadACIHzUnbsPQ0FDt34Su4UXF46VFI0+4wjMDNszdoDYocu0 \
			 ip",
		);
		assert_eq!(cmd.name, "clientinitiv");
	}

	#[test]
	fn clientinitiv2() {
		let cmd = parse_command("clientinitiv alpha=giGMvmfHzbY3ig== omega=MEsDAgcAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITsC/50= ot=1 ip").unwrap();
		assert_eq!(cmd.name, "clientinitiv");
	}

	#[test]
	fn initserver() {
		// Well, that's more corrupted packet, but the parser should be robust
		let s =
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
			 virtualserver_priority_speaker_dimm_modificat";
		let cmd = test_loop(s);
		assert_eq!(cmd.name, "initserver");
	}

	#[test]
	fn channellist() {
		let s =
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
			 channel_flag_private=0"; //|cid=6 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s2\\s\\p\\sThe\\sBook\\sof\\sHeavy\\sMetal channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=4 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=Not\\senought\\sChannels channel_icon_id=0 channel_flag_private=0|cid=30 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s3\\s\\p\\sSenpai\\sGef\xc3\xa4hrlich channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=6 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=The\\strashcan\\shas\\sthe\\strash channel_icon_id=0 channel_flag_private=0";
		let cmd = test_loop(s);
		assert_eq!(cmd.name, "channellist");
	}

	#[test]
	fn subscribe() {
		let s = "notifychannelsubscribed cid=2|cid=4 es=3867|cid=5 \
		         es=18694|cid=6 es=18694|cid=7 es=18694|cid=11 \
		         es=18694|cid=13 es=18694|cid=14 es=18694|cid=16 \
		         es=18694|cid=22 es=18694|cid=23 es=18694|cid=24 \
		         es=18694|cid=25 es=18694|cid=30 es=18694|cid=163 es=18694";
		let cmd = test_loop(s);
		assert_eq!(cmd.name, "notifychannelsubscribed");
	}

	#[test]
	fn permissionlist() {
		let s = "notifypermissionlist group_id_end=0|group_id_end=7|group_id_end=13|group_id_end=18|group_id_end=21|group_id_end=21|group_id_end=33|group_id_end=47|group_id_end=77|group_id_end=82|group_id_end=83|group_id_end=106|group_id_end=126|group_id_end=132|group_id_end=143|group_id_end=151|group_id_end=160|group_id_end=162|group_id_end=170|group_id_end=172|group_id_end=190|group_id_end=197|group_id_end=215|group_id_end=227|group_id_end=232|group_id_end=248|permname=b_serverinstance_help_view permdesc=Retrieve\\sinformation\\sabout\\sServerQuery\\scommands|permname=b_serverinstance_version_view permdesc=Retrieve\\sglobal\\sserver\\sversion\\s(including\\splatform\\sand\\sbuild\\snumber)|permname=b_serverinstance_info_view permdesc=Retrieve\\sglobal\\sserver\\sinformation|permname=b_serverinstance_virtualserver_list permdesc=List\\svirtual\\sservers\\sstored\\sin\\sthe\\sdatabase";
		let cmd = test_loop(s);
		assert_eq!(cmd.name, "notifypermissionlist");
	}

	#[test]
	fn serverquery_command() {
		let s = "cmd=1 cid=2";
		let cmd = test_loop(s);
		assert_eq!(cmd.name, "");
		assert_eq!(cmd.static_args, vec![
			("cmd", "1".into()),
			("cid", "2".into()),
		]);
		assert!(cmd.list_args.is_empty());
	}

	#[test]
	fn no_serverquery_command() {
		let s = "channellistfinished";
		let cmd = test_loop(s);
		assert_eq!(cmd.name, "channellistfinished");
		assert!(cmd.static_args.is_empty());
		assert!(cmd.list_args.is_empty());
	}

	#[test]
	fn newline_command() {
		let s = "sendtextmessage text=\\nmess\\nage\\n return_code=11";
		let cmd = test_loop(s);
		assert_eq!(cmd.name, "sendtextmessage");
	}
}

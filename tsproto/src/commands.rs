use std::io::prelude::*;
use std::str;

use nom::{self, alphanumeric, multispace};

use {Map, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Command {
    pub command: String,
    pub static_args: Vec<(String, String)>,
    pub list_args: Vec<Vec<(String, String)>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalCommand<'a> {
    pub command: &'a str,
    pub args: Map<&'a str, &'a str>,
}

macro_rules! parse_to_string {
    ($x:expr) => {
        String::from_utf8($x.iter().flat_map(|v| v.iter())
            .cloned().collect()).unwrap()
    };
}

named!(command_arg<&[u8], (String, String)>, do_parse!(many0!(multispace) >>
        name: many1!(is_not!("\u{b}\u{c}\\\t\r\n| /=")) >> // Argument name
        value: alt!(
            map!(eof!(), |_| vec![]) |
            do_parse!( // Argument value
                tag!("=") >>
                value: many0!(alt!(
                    map!(tag!("\\v"), |_| &b"\x0b"[..]) | // Vertical tab
                    map!(tag!("\\f"), |_| &b"\x0c"[..]) | // Form feed
                    map!(tag!("\\\\"), |_| &b"\\"[..]) |
                    map!(tag!("\\t"), |_| &b"\t"[..]) |
                    map!(tag!("\\r"), |_| &b"\r"[..]) |
                    map!(tag!("\\n"), |_| &b"\n"[..]) |
                    map!(tag!("\\p"), |_| &b"|"[..]) |
                    map!(tag!("\\s"), |_| &b" "[..]) |
                    map!(tag!("\\/"), |_| &b"/"[..]) |
                    is_not!("\u{b}\u{c}\\\t\r\n| /")
                )) >>
                (value))
            | map!(tag!(""), |_| vec![])
        ) >> (parse_to_string!(name), parse_to_string!(value))
));

named!(parse_command<&[u8], Command>, do_parse!(
    command: many1!(alphanumeric) >> // Command
    static_args: many0!(command_arg) >>
    list_args: many0!(do_parse!(many0!(multispace) >>
        tag!("|") >>
        args: many1!(command_arg) >>
        (args)
    )) >>
    many0!(multispace) >>
    eof!() >>
    (Command {
        command: parse_to_string!(command),
        static_args,
        list_args,
    })
));

impl Command {
    pub fn new<T: Into<String>>(command: T) -> Command {
        Command {
            command: command.into(),
            static_args: Vec::new(),
            list_args: Vec::new(),
        }
    }

    pub fn push<K: Into<String>, V: Into<String>>(&mut self, key: K, val: V) {
        self.static_args.push((key.into(), val.into()));
    }

    /// Replace an argument if it exists.
    pub fn replace<K: Into<String>, V: Into<String>>(
        &mut self,
        key: K,
        val: V,
    ) {
        let key = key.into();
        for &mut (ref mut k, ref mut v) in &mut self.static_args {
            if key == *k {
                *v = val.into();
                break;
            }
        }
    }

    /// Remove an argument if it exists.
    pub fn remove<K: Into<String>>(
        &mut self,
        key: K,
    ) {
        let key = key.into();
        self.static_args.retain(|&(ref k, _)| *k != key);
    }

    /// Check, if each list argument is contained in each list.
    pub fn is_valid(&self) -> bool {
        if !self.list_args.is_empty() {
            let first = &self.list_args[0];
            for l in &self.list_args[1..] {
                if l.len() != first.len() {
                    return false;
                }
                for &(ref arg, _) in first {
                    if l.iter().any(|&(ref a, _)| a == arg) {
                        return false;
                    }
                }
            }
        }
        true
    }

    pub fn read<T>(_: T, r: &mut Read) -> Result<Command> {
        let mut buf = Vec::new();
        r.read_to_end(&mut buf)?;
        // Check if the buffer contains valid UTF-8
        str::from_utf8(&buf)?;
        match parse_command(&buf) {
            nom::IResult::Done(_, mut cmd) => {
                // Some of the static args are variable so move the to the right
                // category.
                if !cmd.list_args.is_empty() {
                    let mut la = Vec::new();
                    for &(ref arg, _) in &cmd.list_args[0] {
                        if let Some(i) = cmd.static_args
                            .iter()
                            .position(|&(ref k, _)| k == arg)
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
            error => Err(format!("Cannot parse a command: {:?}", error).into()),
        }
    }

    fn write_escaped(w: &mut Write, s: &str) -> Result<()> {
        for c in s.chars() {
            match c {
                '\u{b}' => write!(w, "\\v"),
                '\u{c}' => write!(w, "\\f"),
                '\\' => write!(w, "\\\\"),
                '\t' => write!(w, "\\t"),
                '\r' => write!(w, "\\r"),
                '\n' => write!(w, "\\n"),
                '|' => write!(w, "\\p"),
                ' ' => write!(w, "\\s"),
                '/' => write!(w, "\\/"),
                c => write!(w, "{}", c),
            }?;
        }
        Ok(())
    }

    fn write_key_val(w: &mut Write, k: &str, v: &str) -> Result<()> {
        if v.is_empty() && k != "return_code" {
            write!(w, "{}", k)?;
        } else {
            write!(w, "{}=", k)?;
            Self::write_escaped(w, v)?;
        }
        Ok(())
    }

    pub fn write(&self, w: &mut Write) -> Result<()> {
        w.write_all(self.command.as_bytes())?;
        for &(ref k, ref v) in &self.static_args {
            write!(w, " ")?;
            Self::write_key_val(w, k, v)?;
        }
        for (i, args) in self.list_args.iter().enumerate() {
            if i != 0 {
                write!(w, "|")?;
            }
            for (j, &(ref k, ref v)) in args.iter().enumerate() {
                if j != 0 || i == 0 {
                    write!(w, " ")?;
                }
                Self::write_key_val(w, k, v)?;
            }
        }
        Ok(())
    }

    pub fn has_arg(&self, arg: &str) -> bool {
        if self.static_args.iter().any(|&(ref a, _)| a == arg) {
            true
        } else if !self.list_args.is_empty() {
            self.list_args[0].iter().any(|&(ref a, _)| a == arg)
        } else {
            false
        }
    }

    pub fn get_static_arg<K: AsRef<str>>(&self, key: K) -> Option<&str> {
        let key = key.as_ref();
        self.static_args.iter().filter_map(|&(ref k, ref v)| if k == key {
            Some(v.as_str())
        } else {
            None
        }).next()
    }

    pub fn get_commands(&self) -> Vec<CanonicalCommand> {
        let mut res = Vec::new();
        let statics = self.static_args
            .iter()
            .map(|&(ref k, ref v)| (k.as_str(), v.as_str()))
            .collect();
        if self.list_args.is_empty() {
            res.push(CanonicalCommand {
                command: &self.command,
                args: statics,
            });
        } else {
            for l in &self.list_args {
                let mut v = statics.clone();
                v.extend(
                    l.iter().map(|&(ref k, ref v)| (k.as_str(), v.as_str())),
                );
                res.push(CanonicalCommand {
                    command: &self.command,
                    args: v,
                });
            }
        }
        res
    }
}

impl<'a> CanonicalCommand<'a> {
    pub fn has_arg(&self, arg: &str) -> bool {
        self.args.contains_key(arg)
    }
}

#[cfg(test)]
mod tests {
    use io::Cursor;
    use std::iter::FromIterator;

    use Map;
    use commands::{CanonicalCommand, Command};

    #[test]
    fn parse() {
        let s = b"cmd a=1 b=2 c=3";
        let mut cmd = Command::new("cmd");
        cmd.push("a", "1");
        cmd.push("b", "2");
        cmd.push("c", "3");

        // Read
        let cmd_r = Command::read((), &mut Cursor::new(s)).unwrap();
        assert_eq!(cmd, cmd_r);
        // Write
        let mut s_r = Vec::new();
        cmd.write(&mut s_r).unwrap();
        assert_eq!(&s[..], s_r.as_slice());
    }

    #[test]
    fn escape() {
        let s = b"cmd a=\\s\\\\ b=\\p c=abc\\tdef";
        let mut cmd = Command::new("cmd");
        cmd.push("a", " \\");
        cmd.push("b", "|");
        cmd.push("c", "abc\tdef");

        // Read
        let cmd_r = Command::read((), &mut Cursor::new(s)).unwrap();
        assert_eq!(cmd, cmd_r);
        // Write
        let mut s_r = Vec::new();
        cmd.write(&mut s_r).unwrap();
        assert_eq!(&s[..], s_r.as_slice());
    }

    #[test]
    fn array() {
        let s = b"cmd a=1 c=3 b=2|b=4|b=5";
        let mut cmd = Command::new("cmd");
        cmd.push("a", "1");
        cmd.push("c", "3");

        cmd.list_args.push(vec![("b".into(), "2".into())]);
        cmd.list_args.push(vec![("b".into(), "4".into())]);
        cmd.list_args.push(vec![("b".into(), "5".into())]);

        // Read
        let cmd_r = Command::read((), &mut Cursor::new(s)).unwrap();
        assert_eq!(cmd, cmd_r);
        // Write
        let mut s_r = Vec::new();
        cmd.write(&mut s_r).unwrap();
        assert_eq!(&s[..], s_r.as_slice());
    }

    #[test]
    fn canonical_command() {
        let s = b"cmd a=1 b=2 c=3|b=4|b=5";
        let cmd = Command::read((), &mut Cursor::new(s)).unwrap();
        assert_eq!(
            cmd.get_commands(),
            vec![
                CanonicalCommand {
                    command: "cmd",
                    args: Map::from_iter(
                        vec![("a", "1"), ("b", "2"), ("c", "3")]
                            .iter()
                            .cloned(),
                    ),
                },
                CanonicalCommand {
                    command: "cmd",
                    args: Map::from_iter(
                        vec![("a", "1"), ("b", "4"), ("c", "3")]
                            .iter()
                            .cloned(),
                    ),
                },
                CanonicalCommand {
                    command: "cmd",
                    args: Map::from_iter(
                        vec![("a", "1"), ("b", "5"), ("c", "3")]
                            .iter()
                            .cloned(),
                    ),
                },
            ]
        );
    }

    #[test]
    fn optional_arg() {
        let s = b"cmd a";
        Command::read((), &mut Cursor::new(s.as_ref())).unwrap();
        let s = b"cmd a b=1";
        Command::read((), &mut Cursor::new(s.as_ref())).unwrap();
        let s = b"cmd a=";
        Command::read((), &mut Cursor::new(s.as_ref())).unwrap();
        let s = b"cmd a= b=1";
        Command::read((), &mut Cursor::new(s.as_ref())).unwrap();
    }

    #[test]
    fn clientinitiv() {
        let s = b"clientinitiv alpha=41Te9Ar7hMPx+A== omega=MEwDAgcAAgEgAiEAq2iCMfcijKDZ5tn2tuZcH+\\/GF+dmdxlXjDSFXLPGadACIHzUnbsPQ0FDt34Su4UXF46VFI0+4wjMDNszdoDYocu0 ip";
        Command::read((), &mut Cursor::new(s.as_ref())).unwrap();
    }

    #[test]
    fn initserver() {
        // Well, that's more corrupted packet, but the parser should be robust
        let s = b"initserver virtualserver_name=Server\\sder\\sVerplanten virtualserver_welcomemessage=This\\sis\\sSplamys\\sWorld virtualserver_platform=Linux virtualserver_version=3.0.13.8\\s[Build:\\s1500452811] virtualserver_maxclients=32 virtualserver_created=0 virtualserver_nodec_encryption_mode=1 virtualserver_hostmessage=L\xc3\xa9\\sServer\\sde\\sSplamy virtualserver_name=Server_mode=0 virtualserver_default_server group=8 virtualserver_default_channel_group=8 virtualserver_hostbanner_url virtualserver_hostmessagegfx_url virtualserver_hostmessagegfx_interval=2000 virtualserver_priority_speaker_dimm_modificat";
        Command::read((), &mut Cursor::new(s.as_ref())).unwrap();
    }

    #[test]
    fn channellist() {
        let s = b"channellist cid=2 cpid=0 channel_name=Trusted\\sChannel channel_topic channel_codec=0 channel_codec_quality=0 channel_maxclients=0 channel_maxfamilyclients=-1 channel_order=1 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=0 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic channel_icon_id=0 channel_flag_private=0|cid=4 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s1\\s\\p\\sSplamy\xc2\xb4s\\sBett channel_topic channel_codec=4 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=0 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=Neo\\sSeebi\\sEvangelion channel_icon_id=0 channel_flag_private=0"; //|cid=6 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s2\\s\\p\\sThe\\sBook\\sof\\sHeavy\\sMetal channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=4 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=Not\\senought\\sChannels channel_icon_id=0 channel_flag_private=0|cid=30 cpid=2 channel_name=Ding\\s\xe2\x80\xa2\\s3\\s\\p\\sSenpai\\sGef\xc3\xa4hrlich channel_topic channel_codec=2 channel_codec_quality=7 channel_maxclients=-1 channel_maxfamilyclients=-1 channel_order=6 channel_flag_permanent=1 channel_flag_semi_permanent=0 channel_flag_default=0 channel_flag_password=0 channel_codec_latency_factor=1 channel_codec_is_unencrypted=1 channel_delete_delay=0 channel_flag_maxclients_unlimited=1 channel_flag_maxfamilyclients_unlimited=0 channel_flag_maxfamilyclients_inherited=1 channel_needed_talk_power=0 channel_forced_silence=0 channel_name_phonetic=The\\strashcan\\shas\\sthe\\strash channel_icon_id=0 channel_flag_private=0";
        Command::read((), &mut Cursor::new(s.as_ref())).unwrap();
    }

    #[test]
    fn subscribe() {
        let s = b"notifychannelsubscribed cid=2|cid=4 es=3867|cid=5 es=18694|cid=6 es=18694|cid=7 es=18694|cid=11 es=18694|cid=13 es=18694|cid=14 es=18694|cid=16 es=18694|cid=22 es=18694|cid=23 es=18694|cid=24 es=18694|cid=25 es=18694|cid=30 es=18694|cid=163 es=18694";
        Command::read((), &mut Cursor::new(s.as_ref())).unwrap();
    }
}

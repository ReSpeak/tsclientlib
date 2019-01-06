use std::borrow::Cow;
use std::cell::RefCell;
use std::path::Path;
use std::sync::Mutex;

use lazy_static::lazy_static;
use rustyline::Editor;
use rustyline::completion::Completer;
use rustyline::config::{CompletionType, Config};
use rustyline::error::ReadlineError;
use rustyline::highlight::Highlighter;
use rustyline::hint::Hinter;
use termion::{color, style};
use tsproto::commands::Command;
use tsproto_structs::messages;

const PROMPT: &str = "> ";

const SPLIT_CHARS: &[char] = &[' ', '\t', '|'];

// Hacky workaround for windows
struct EditorWrapper(Editor<CommandHelper>);
unsafe impl Send for EditorWrapper {}

fn colored_prompt() -> String {
    format!("{}{}{}{}{}", style::Bold, color::Fg(color::Green), PROMPT, color::Fg(color::Reset), style::Reset)
}
fn hint(s: &str) -> String {
    format!("{}{}{}", color::Fg(color::LightBlack), s, color::Fg(color::Reset))
}
fn command(s: &str) -> String {
    format!("{}{}{}{}{}", style::Bold, color::Fg(color::Yellow), s, color::Fg(color::Reset), style::Reset)
}
fn digit(s: &str) -> String {
    format!("{}{}{}", color::Fg(color::LightCyan), s, color::Fg(color::Reset))
}
fn eq(s: &str) -> String {
    s.into()
}
fn error(s: &str) -> String {
    format!("{}{}{}", color::Fg(color::Red), s, color::Fg(color::Reset))
}
fn escape_code(s: &str) -> String {
    format!("{}{}{}", color::Fg(color::LightBlue), s, color::Fg(color::Reset))
}
fn list_arg(s: &str) -> String {
    format!("{}{}{}{}{}", style::Bold, color::Fg(color::Blue), s, color::Fg(color::Reset), style::Reset)
}
fn pipe(s: &str) -> String {
    format!("{}{}{}", style::Bold, s, style::Reset)
}
fn static_arg(s: &str) -> String {
    format!("{}{}{}{}{}", style::Bold, color::Fg(color::Magenta), s, color::Fg(color::Reset), style::Reset)
}
fn value(s: &str) -> String {
    s.into()
}

lazy_static! {
    static ref EDITOR: Mutex<EditorWrapper> = {
        let mut rl = Editor::<CommandHelper>::with_config(Config::builder()
            .completion_type(CompletionType::List)
            .auto_add_history(true)
            .build());
        rl.set_helper(Some(CommandHelper::new()));
        Mutex::new(EditorWrapper(rl))
    };
}

pub fn load_history<P: AsRef<Path> + ?Sized>(path: &P) -> rustyline::Result<()> {
    EDITOR.lock().unwrap().0.load_history(path)
}

pub fn save_history<P: AsRef<Path> + ?Sized>(path: &P) -> rustyline::Result<()> {
    EDITOR.lock().unwrap().0.save_history(path)
}

struct CommandHelper {
    last_command_name: RefCell<String>,
}

impl CommandHelper {
    fn new() -> Self {
        Self {
            last_command_name: RefCell::new(String::new()),
        }
    }

    fn highlight_str<'s>(&self, s: &'s str) -> String {
        let mut res = String::new();
        let mut last_num = false;
        let mut last_escape_char = false;
        let mut in_escape = false;
        let mut last_part = String::new();
        for c in s.chars() {
            if last_num && !c.is_digit(10) {
                res.push_str(&digit(&last_part));
                last_part.clear();
                last_num = false;
            }
            if c == '\x1b' {
                in_escape = true;
            }
            match c {
                '=' => {
                    res.push_str(&eq("="));
                }
                '|' => {
                    res.push_str(&pipe("|"));
                }
                '\\' => {
                    last_part.push(c);
                }
                _ if c.is_digit(10) && !in_escape => {
                    last_num = true;
                    last_part.push(c);
                }
                _ if !last_part.is_empty() => {
                    last_part.push(c);
                    res.push_str(&escape_code(&last_part));
                    last_part.clear();
                }
                _ => res.push(c),
            }
            if last_escape_char {
                last_escape_char = false;
            } else if c == '\\' {
                last_escape_char = true;
            }
            if in_escape && c == 'm' {
                in_escape = false;
            }
        }
        if last_num {
            res.push_str(&digit(&last_part));
        } else if !last_part.is_empty() {
            res.push_str(&error(&last_part));
        }
        res
    }

    fn highlight_command<'s>(&self, s: &'s str) -> String {
        let mut parts: Vec<_> = s.split(SPLIT_CHARS).collect();
        let mut pos = 0;

        let mut res = String::new();
        if !parts.is_empty() && !parts[0].contains('=') {
            // Has command
            res = command(parts[0]);
            // Delimiter
            pos += parts[0].len();
            if let Some(del) = s.chars().nth(pos) {
                if del == '|' {
                    res.push_str(&error("|"));
                } else {
                    res.push(del);
                }
                pos += 1;
            }
            parts.remove(0);
        }

        // Arguments:
        // Name
        // Value (including =)
        // Following delimiter
        let mut list_args = Vec::<Vec<(&str, &str, Option<char>)>>::new();
        list_args.push(Vec::new());
        for p in parts {
            let (name, val) = if let Some(pos) = p.find('=') {
                (&p[..pos], &p[pos..])
            } else {
                (p, "")
            };

            // Delimiter
            pos += p.len();
            let del = s.chars().nth(pos);
            if del.is_some() {
                pos += 1;
                if del == Some('|') {
                    list_args.push(Vec::new());
                }
            }
            list_args.last_mut().unwrap().push((name, val, del));
        }

        // Get the name of the static arguments
        let mut static_args;
        if list_args.len() == 1 {
            static_args = list_args[0].iter().map(|a| a.0).collect();
        } else if list_args.len() >= 2 {
            static_args = Vec::new();
            let a = &list_args[0];
            let b = &list_args[1];
            for a in a {
                let name = a.0;
                if b.iter().filter(|arg| arg.0 == name).next().is_none() {
                    static_args.push(a.0);
                }
            }
        } else {
            static_args = Vec::new();
        }

        // Print
        for group in &list_args {
            for (name, val, del) in group {
                let is_static = static_args.contains(&name);
                // Name
                if is_static {
                    res.push_str(&static_arg(&name));
                } else {
                    res.push_str(&list_arg(&name));
                }

                // Value
                if !val.is_empty() {
                    if val.starts_with('=') {
                        res.push_str(&eq("="));
                        res.push_str(&value(&val[1..]));
                    } else {
                        res.push_str(&value(val));
                    }
                }

                if let Some(del) = del {
                    res.push(*del);
                }
            }
        }

        self.highlight_str(&res)
    }

    fn get_command_name<'s>(&self, s: &'s str) -> Option<&'s str> {
        s.split(SPLIT_CHARS).next().and_then(|s|
            if s.contains('=') { None } else { Some(s) })
    }
}

impl rustyline::Helper for CommandHelper {}

impl Completer for CommandHelper {
    type Candidate = String;
    // Return start position and completion
    fn complete(
        &self,
        s: &str,
        pos: usize
    ) -> rustyline::Result<(usize, Vec<Self::Candidate>)> {
        let s = &s[..pos];
        if pos == s.len() {
            // Complete the hint if we have one
            if let Some(hint) = self.hint(s, pos) {
                return Ok((pos, vec![hint]));
            }
        }

        if let Some(pos) = s.rfind(SPLIT_CHARS) {
            // Complete arguments
            let command_name = self.get_command_name(s);
            if let Some((_command_name, msg)) = command_name
                .and_then(|c| messages::DATA.msg_group.iter()
                    .flat_map(|g| g.msg.iter())
                    .find(|msg| msg.notify.as_ref().map(|s| s == c).unwrap_or(false))
                    .map(|r| (c, r))) {
                // Check where we end in an argument
                let part = &s[pos + 1..];
                if !part.contains('=') {
                    // Name
                    let vals = msg.attributes.iter()
                        .map(|a| messages::DATA.get_field(a))
                        .filter_map(|f| if f.ts.starts_with(part) { Some(f.ts.clone()) }
                            else { None });
                    // TODO Wrong position
                    return Ok((s.len() - part.len(), vals.collect()));
                } else {
                    // Value
                    // TODO Complete enums
                }
            }
            return Ok((pos, Vec::new()));
        }

        // Complete commands
        let possible_cmds = messages::DATA.msg_group.iter()
            .flat_map(|g| g.msg.iter())
            .filter_map(|msg| msg.notify.as_ref().and_then(|msg|
                if msg.starts_with(s) { Some(format!("{} ", msg)) } else { None }));
        return Ok((0, possible_cmds.collect()));
    }
}

impl Hinter for CommandHelper {
    fn hint(&self, s: &str, _pos: usize) -> Option<String> {
        if s.is_empty() {
            return None;
        }
        if let Some(pos) = s.rfind(SPLIT_CHARS) {
            // Complete arguments
            let command_name = self.get_command_name(s);
            if let Some((_command_name, msg)) = command_name
                .and_then(|c| messages::DATA.msg_group.iter()
                    .flat_map(|g| g.msg.iter())
                    .find(|msg| msg.notify.as_ref().map(|s| s == c).unwrap_or(false))
                    .map(|r| (c, r))) {
                // Check where we end in an argument
                let part = &s[pos + 1..];
                if !part.contains('=') {
                    // Name
                    let mut vals = msg.attributes.iter()
                        .map(|a| messages::DATA.get_field(a))
                        .filter_map(|f| if f.ts.starts_with(part) { Some(&f.ts) }
                            else { None });
                    if let Some(arg) = vals.next() {
                        if vals.next().is_none() {
                            // Only one argument left
                            return Some(arg[part.len()..].to_string());
                        }
                    }
                } else {
                    // Value
                }
            }
            return None;
        }

        let mut possible_cmds = messages::DATA.msg_group.iter()
            .flat_map(|g| g.msg.iter())
            .filter_map(|msg| msg.notify.as_ref().and_then(|msg|
                if msg.starts_with(s) { Some(msg) } else { None }));
        if let Some(cmd) = possible_cmds.next() {
            if possible_cmds.next().is_none() {
                // Only one command left
                return Some(cmd[s.len()..].to_string());
            }
        }
        None
    }
}

impl Highlighter for CommandHelper {
    fn highlight<'s>(&self, s: &'s str, _pos: usize) -> Cow<'s, str> {
        if s.is_empty() {
            return Cow::Borrowed(s);
        }

        // Parse command
        let command_name = self.get_command_name(s);

        // Try to find the command
        if let Some((command_name, msg)) = command_name
            .and_then(|c| messages::DATA.msg_group.iter()
                .flat_map(|g| g.msg.iter())
                .find(|msg| msg.notify.as_ref().map(|s| s == c).unwrap_or(false))
                .map(|r| (c, r))) {
            if *self.last_command_name.borrow() != command_name {
                // Print message
                let mut cmd = Command::new(command_name);
                for attr in &msg.attributes {
                    let field = messages::DATA.get_field(attr);
                    let attr_name = if attr.ends_with('?') {
                        format!("{}?", field.ts)
                    } else {
                        field.ts.to_string()
                    };

                    let type_s = match &field.modifier {
                        None => field.type_s.to_string(),
                        Some(m) if m == "array" => format!("[{}]", field.type_s),
                        Some(modifier) => {
                            eprintln!("Unknown field modifier {}", modifier);
                            field.type_s.to_string()
                        }
                    };
                    cmd.push(attr_name, format!("<{}>", type_s));
                }

                // Print
                let mut buf = Vec::new();
                cmd.write(&mut buf).unwrap();
                let res = self.highlight_command(std::str::from_utf8(&buf).unwrap());
                // TODO Don't use println
                println!("\r{}", res);
                self.last_command_name.replace(command_name.into());
            }
        }

        let res = self.highlight_command(s);
        Cow::Owned(self.highlight_str(&res))
    }

    fn highlight_prompt<'s>(&self, s: &'s str) -> Cow<'s, str> {
        if s == PROMPT {
            Cow::Owned(colored_prompt())
        } else {
            Cow::Borrowed(s)
        }
    }

    fn highlight_hint<'s>(&self, s: &'s str) -> Cow<'s, str> {
        Cow::Owned(hint(s))
    }
}

pub fn read_command() -> Option<Command> {
    let mut rl = EDITOR.lock().unwrap();
    loop {
        let readline = rl.0.readline("> ");
        match readline {
            Ok(line) => {
                if line.is_empty() {
                    continue;
                }
                if line.starts_with('!') {
                    // Send as verbatim command
                    return Some(Command::new(line));
                }

                let cmd = Command::read((), &mut line.as_bytes());
                match cmd {
                    Ok(c) => return Some(c),
                    Err(e) => {
                        println!("Failed to parse command: {:?}", e);
                    }
                }
            },
            Err(ReadlineError::Interrupted) => {
                return None;
            },
            Err(ReadlineError::Eof) => {
                return None;
            },
            Err(err) => {
                println!("Error: {:?}", err);
                return None;
            }
        }
    }
}

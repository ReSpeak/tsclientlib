#![allow(dead_code)] // TODO Remove

use std::fmt;
use std::fs;
use std::io::Write;
use std::net::SocketAddr;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;

use chrono::{DateTime, Utc};
use crossterm::{style, StyledObject};
use failure::format_err;
use futures::{Future, Sink, Stream};
use futures::sync::mpsc;
use lazy_static::lazy_static;
use serde_derive::{Deserialize, Serialize};
use slog::{error, Drain, KV, Level, Logger, o};
use structopt::clap::AppSettings;
use structopt::StructOpt;
use tsclientlib::{ConnectOptions, Connection, DisconnectOptions, Reason};
use tsclientlib::messages::Message;
use tsproto::commands::Command;
use tsproto::handler_data::{PacketObserver, UdpPacketObserver};
use tsproto::packets::{self, Header, Packet, PacketType};

const SETTINGS_FILENAME: &str = "settings.toml";
const KEY_FILENAME: &str = "private.key";
const HISTORY_FILENAME: &str = "history.txt";

type Result<T> = std::result::Result<T, failure::Error>;

mod colors;
mod table_widget;
mod text_widget;
mod ui;

use crate::colors::*;
use crate::table_widget::{Entry, EntryWidget};
use crate::ui::{ui, UiEvent};

#[derive(StructOpt, Debug)]
#[structopt(raw(
	global_settings = "&[AppSettings::ColoredHelp, \
	                   AppSettings::VersionlessSubcommands]"
))]
struct Args {
	#[structopt(
		short = "a",
		long = "address",
		default_value = "localhost",
		help = "The address of the server to connect to"
	)]
	address: String,
	#[structopt(
		long = "settings",
		help = "The settings file"
	)]
	settings: Option<String>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Settings {
}

#[allow(dead_code)] // TODO Remove
struct State {
	logger: Logger,
	proj_dirs: directories::ProjectDirs,
	config_file: PathBuf,
	history_file: PathBuf,

	ui_sender: std::sync::mpsc::Sender<UiEvent>,
	ui_recv: Option<std::sync::mpsc::Receiver<UiEvent>>,
	command_sender: mpsc::UnboundedSender<Option<Command>>,
	command_recv: Option<mpsc::UnboundedReceiver<Option<Command>>>,

	args: Args,
	settings: Settings,
	private_key: Option<Vec<u8>>,

	con: Option<Connection>,

	// Recorded
	log_records: Vec<LogRecord>,
	messages: Vec<MessageRecord>,
	packets: Vec<PacketRecord>,
	udp_packets: Vec<UdpPacketRecord>,
	entries: Vec<EntryWidget>,
}

#[derive(Clone, Debug)]
struct RecordBase {
	/// If this object is incoming or outgoing.
	incoming: bool,
	/// When this record was sent.
	time: DateTime<Utc>,
	/// The type, if this record is a packet.
	p_type: Option<PacketType>,
}

trait Record: Deref<Target=RecordBase> {
}

#[derive(Clone)]
struct LogRecord {
	time: DateTime<Utc>,
	data: Vec<StyledObject<String>>,
}
#[derive(Clone, Debug)]
struct MessageRecord {
	base: RecordBase,
	data: Message,
}
#[derive(Clone, Debug)]
struct PacketRecord {
	base: RecordBase,
	data: Packet,
}
#[derive(Clone, Debug)]
struct UdpPacketRecord {
	base: RecordBase,
	data: Vec<u8>,
}

lazy_static! {
	static ref STATE: Mutex<State> = {
		// Parse command line options
		let args = Args::from_args();

		let logger = {
			//let decorator = slog_term::TermDecorator::new().build();
			//let drain = slog_term::CompactFormat::new(decorator).build().fuse();
			let drain = LogRecorder.fuse();
			let drain = slog_async::Async::new(drain).build().fuse();

			slog::Logger::root(drain, o!())
		};

		// Load settings
		let proj_dirs = match directories::ProjectDirs::from("", "ReSpeak",
			"ts-dev-client") {
			Some(r) => r,
			None => {
				panic!("Failed to get project directory");
			}
		};
		let config_file = args.settings.as_ref().map(PathBuf::from)
			.unwrap_or_else(|| proj_dirs.config_dir().join(SETTINGS_FILENAME));
		let settings = match fs::read_to_string(&config_file) {
			Ok(r) => toml::from_str(&r).unwrap(),
			Err(e) => {
				error!(logger, "Failed to read settings"; "error" => ?e);
				Settings::default()
			}
		};

		// Load history
		if let Err(e) = fs::create_dir_all(proj_dirs.data_dir()) {
			error!(logger, "Failed to create data dictionary"; "error" => ?e);
		}
		let history_file = proj_dirs.data_dir().join(HISTORY_FILENAME);
		//if let Err(e) = ts_cli_completer::load_history(&history_file) {
			//error!(logger, "Failed to load the history"; "error" => ?e);
		//}

		let (ui_sender, ui_recv) = std::sync::mpsc::channel();
		let ui_recv = Some(ui_recv);
		let (command_sender, command_recv) = mpsc::unbounded();
		let command_recv = Some(command_recv);

		Mutex::new(State {
			logger, proj_dirs, config_file, history_file,
			args, settings,
			private_key: Default::default(),

			ui_sender, ui_recv,
			command_sender, command_recv,

			con: Default::default(),

			log_records: Default::default(),
			messages: Default::default(),
			packets: Default::default(),
			udp_packets: Default::default(),
			entries: Default::default(),
		})
	};
}

struct PacketRecorder {
	incoming: bool,
}
impl<T: Send> PacketObserver<T> for PacketRecorder {
	fn observe(&self, _: &mut (T, tsproto::connection::Connection), packet: &mut Packet) {
		let p_type = packet.header.get_type();
		let mut state = STATE.lock().unwrap();
		let base = RecordBase::new(self.incoming, p_type);
		state.packets.push(PacketRecord {
			base: base.clone(),
			data: packet.clone(),
		});
		//state.add_entry(Entry::new_packet)
		match &packet.data {
			packets::Data::Command(cmd) | packets::Data::CommandLow(cmd) => {
				for c in cmd.get_commands() {
					match Message::parse(c) {
						Ok(msg) => {
							state.add_entry(Entry::new_message(MessageRecord {
								base: base.clone(),
								data: msg,
							}));
						}
						Err(e) => {
							error!(state.logger, "Failed to parse message";
								"error" => ?e);
						}
					}
				}
				let _ = state.ui_sender.send(UiEvent::Message);
			}
			_ => {
				let _ = state.ui_sender.send(UiEvent::Packet);
			}
		}
	}
}

struct UdpPacketRecorder {
	incoming: bool,
}
impl UdpPacketObserver for UdpPacketRecorder {
	fn observe(&self, _: SocketAddr, udp_packet: &[u8]) {
		let mut state = STATE.lock().unwrap();
		// TODO Get packet type
		state.udp_packets.push(UdpPacketRecord {
			base: RecordBase::new(self.incoming, None),
			data: udp_packet.to_vec(),
		});
		let _ = state.ui_sender.send(UiEvent::UdpPacket);
	}
}

impl Deref for MessageRecord {
	type Target = RecordBase;
	fn deref(&self) -> &Self::Target { &self.base }
}
impl Record for MessageRecord {}

impl Deref for PacketRecord {
	type Target = RecordBase;
	fn deref(&self) -> &Self::Target { &self.base }
}
impl Record for PacketRecord {}

impl Deref for UdpPacketRecord {
	type Target = RecordBase;
	fn deref(&self) -> &Self::Target { &self.base }
}
impl Record for UdpPacketRecord {}

impl RecordBase {
	fn new<P: Into<Option<PacketType>>>(incoming: bool, p_type: P) -> Self {
		Self {
			incoming,
			time: Utc::now(),
			p_type: p_type.into(),
		}
	}
}

struct LogSerializer<'a>(&'a mut Vec<StyledObject<String>>);
impl<'a> slog::Serializer for LogSerializer<'a> {
	fn emit_arguments(&mut self, key: slog::Key, val: &fmt::Arguments) -> slog::Result {
		self.0.push(style(", ".into()));

		let k = style(format!("{}", key));
		#[cfg(unix)]
		let k = k.bold();
		self.0.push(k);
		self.0.push(style(": ".into()).with(FONT_COLOR).on(BACKGROUND_COLOR).no_reset());
		self.0.push(style(format!("{}", val)));
		Ok(())
	}
}

fn format_log_level(level: Level) -> StyledObject<String> {
	let res = style(level.to_string());
	match level {
		Level::Critical => res.with(CRIT_COLOR).no_reset(),
		Level::Error    => res.with(ERRO_COLOR).no_reset(),
		Level::Warning  => res.with(WARN_COLOR).no_reset(),
		Level::Info     => res.with(INFO_COLOR).no_reset(),
		Level::Debug    => res.with(DEBG_COLOR).no_reset(),
		Level::Trace    => res.with(TRAC_COLOR).no_reset(),
	}
}

struct LogRecorder;
impl Drain for LogRecorder {
	type Ok = ();
	type Err = failure::Error;
	fn log(&self, record: &slog::Record, values: &slog::OwnedKVList)
		-> Result<()> {
		let mut state = STATE.lock().unwrap();
		let mut text: Vec<StyledObject<String>> = Vec::new();
		text.push(format_log_level(record.level()));

		let msg = style(format!(" {}", record.msg())).with(FONT_COLOR);
		#[cfg(unix)]
		let msg = msg.bold();
		text.push(msg);
		text.push(style(String::new()).no_reset().with(FONT_COLOR)
			.on(BACKGROUND_COLOR));

		values.serialize(record, &mut LogSerializer(&mut text))?;
		record.kv().serialize(record, &mut LogSerializer(&mut text))?;
		state.add_entry(Entry::new_log(LogRecord {
			time: Utc::now(),
			data: text,
		}));
		let _ = state.ui_sender.send(UiEvent::Log);
		Ok(())
	}
}

impl State {
	fn add_entry(&mut self, entry: Entry) {
		let date = entry.get_date();
		if let Some(e) = self.entries.last() {
			if e.0.get_date() != date {
				self.entries.push(EntryWidget(Entry::NewDate(date)));
			}
		} else {
			self.entries.push(EntryWidget(Entry::NewDate(date)));
		}
		self.entries.push(EntryWidget(entry));
	}
}

fn run() {
	let (logger, recv) = {
		let mut state = STATE.lock().unwrap();
		(state.logger.clone(), state.command_recv.take().unwrap())
	};
	tokio::run(
		futures::lazy(move || {
			let state = STATE.lock().unwrap();
			let args = &state.args;
			let con_config = ConnectOptions::new(args.address.clone())
				.logger(logger.clone())
				.prepare_client(Box::new(|client| {
					let mut client = client.lock().unwrap();
					client.add_udp_packet_observer(true, "dev-client".into(),
						Box::new(UdpPacketRecorder { incoming: true }));
					client.add_udp_packet_observer(false, "dev-client".into(),
						Box::new(UdpPacketRecorder { incoming: false }));
					client.add_packet_observer(true, "dev-client".into(),
						Box::new(PacketRecorder { incoming: true }));
					client.add_packet_observer(false, "dev-client".into(),
						Box::new(PacketRecorder { incoming: false }));
				}));

			let con_config = con_config.private_key_ts(
				"MG0DAgeAAgEgAiAIXJBlj1hQbaH0Eq0DuLlCmH8bl+veTAO2+\
				k9EQjEYSgIgNnImcmKo7ls5mExb6skfK2Tw+u54aeDr0OP1ITs\
				C/50CIA8M5nmDBnmDM/gZ//4AAAAAAAAAAAAAAAAAAAAZRzOI").unwrap();

			if args.settings.is_none() {
				// Write settings
				//if let Err(e) = fs::create_dir_all(proj_dirs.config_dir()) {
					//error!(logger, "Failed to create config dictionary"; "error" => ?e);
				//} else if let Err(e) = fs::write(&config_file,
					//toml::to_string(&settings).unwrap().as_bytes()) {
					//error!(logger, "Failed to write settings"; "error" => ?e);
				//}
			}

			// Connect
			Connection::new(con_config)
		}).and_then(|con| {
			STATE.lock().unwrap().con = Some(con.clone());

			let cmd = Command::new("channelsubscribeall");
			let header = Header::new(PacketType::Command);
			let data = packets::Data::Command(cmd);
			let packet = packets::Packet::new(header, data);

			// Send a message and wait until we get an answer for the return code
			con.get_packet_sink().send(packet).map(|_| con)
		}).and_then(|con| {

			// Wait for commands
			let con2 = con.clone();
			recv.take_while(|c| Ok(c.is_some()))
				.map(|c| c.unwrap())
				.map_err(|e| format_err!("Error reading command ({:?})", e).into())
				.for_each(move |cmd| {
					// Send command
					let header = Header::new(PacketType::Command);
					let data = packets::Data::Command(cmd);
					let packet = packets::Packet::new(header, data);

					con2.get_packet_sink().send(packet).map(|_| ())
				})
				.map(move |_| con)
		}).and_then(|con| {
			// Disconnect
			con.disconnect(
				DisconnectOptions::new()
					.reason(Reason::Clientdisconnect)
					.message("Is this the real world?"),
			)
		}).map_err(|e| panic!("An error occurred {:?}", e)),
	);

	// Save history
	//if let Err(e) = ts_cli_completer::save_history(&STATE.lock().unwrap().history_file) {
		//error!(logger2, "Failed to save the history"; "error" => ?e);
	//}
}

fn set_panic_hook() {
	// Print panics to file
	let hook = std::panic::take_hook();
	std::panic::set_hook(Box::new(move |i| {
		let _ = fs::OpenOptions::new().append(true).open("error.log")
			.and_then(|mut f| {
			write!(f, "Panic: {:?}\n\n", i)
		});
		hook(i);
	}));
}

fn main() -> Result<()> {
	set_panic_hook();
	thread::spawn(run);

	// The ui has to run in the main thread to get events
	ui()
}

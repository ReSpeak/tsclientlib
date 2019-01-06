use std::io::Write;

use chrono::{DateTime, Utc};
use crossterm::{Crossterm, Screen, TerminalColor, TerminalCursor};
use crossterm::style::style;
use crossterm::terminal::ClearType;
use mortal::{Event, Key, MouseEvent, MouseInput, Signal};
use tsproto::commands::Command;
use tsproto::packets;
use unicode_width::UnicodeWidthStr;

use crate::{STATE, Result};
use crate::colors::*;
use crate::table_widget::TableWidget;

/// How many lines to scroll on one scroll event.
const SCROLL_LINES: isize = 10;

#[derive(Eq, Clone, Debug, Hash, PartialEq)]
pub struct Rect {
	pub x: u16,
	pub y: u16,
	pub width: u16,
	pub height: u16,
}

pub trait Widget {
	/// Set the base size of this widget which will be used for further
	/// size computations.
	fn set_size(&mut self, width: u16, height: u16);
	/// The requested width of the widget.
	fn width(&self) -> u16;
	/// The requested height of the widget.
	fn height(&self) -> u16;
	/// The widget writes itself to the terminal into the given `area`.
	fn paint(&self, area: Rect, screen: &mut Screen, cursor: &TerminalCursor,
		color: &mut TerminalColor) -> Result<()>;
}

pub enum UiEvent {
	Log,
	Message,
	Packet,
	UdpPacket,
	Redraw,
	Resize,
	KeyEvent(Key),
	MouseEvent(MouseEvent),
	Quit,
}

pub struct Ui {
	cross: Crossterm,
	alt_screen: crossterm::AlternateScreen,
	width: u16,
	height: u16,
	// If this part of the screen needs to be redrawn.
	table_header_invalid: bool,
	table_invalid: bool,
	details_invalid: bool,
	input_invalid: bool,

	start_time: DateTime<Utc>,
	/// How many lines below the first item we start to draw.
	///
	/// The first item is determined by the start time in the state.
	tables_line_offset: usize,

	table: TableWidget,

	input: String,
	cursor_pos: usize,
}

impl Rect {
	pub fn new(x: u16, y: u16, width: u16, height: u16) -> Self {
		Self { x, y, width, height }
	}
}

impl Ui {
	pub fn new() -> Result<Self> {
		let screen = crossterm::Screen::default();
		// Render to alternate screen buffer
		let alt_screen = screen.enable_alternate_modes(true)?;
		//alt_screen.to_main_screen()?;
		let cross = Crossterm::new(&alt_screen.screen);
		let term = cross.terminal();
		let (width, height) = term.terminal_size();

		Ok(Self {
			cross,
			alt_screen,
			width,
			height,
			table_header_invalid: true,
			table_invalid: true,
			details_invalid: true,
			input_invalid: true,

			start_time: Utc::now(),
			tables_line_offset: 0,

			table: TableWidget::new(),

			input: Default::default(),
			cursor_pos: 0,
		})
	}

	fn draw_table_header(width: u16, screen: &mut Screen,
		cursor: &TerminalCursor, color: &mut TerminalColor) -> Result<()> {
		cursor.goto(0, 0, screen);
		color.set_fg(HEADER_COLOR, screen);

		write!(screen, "{:<13}", "Time")?;
		let mut len = 13;
		write!(screen, "Dir ")?;
		len += 4;
		write!(screen, "Content")?;
		len += 7;

		// Fill the rest with background color
		if width as usize > len {
			write!(screen, "{}", " ".repeat(width as usize - len))?;
		}

		Ok(())
	}

	fn draw_details(width: u16, screen: &mut Screen,
		cursor: &TerminalCursor, color: &mut TerminalColor) -> Result<()> {
		cursor.goto(0, 1, screen);

		let ps = &STATE.lock().unwrap().packets;
		for p in ps {
			match &p.data.data {
				packets::Data::Command(cmd) | packets::Data::CommandLow(cmd) => {
					color.set_fg(FONT_COLOR, screen);
					write!(screen, "{} ", p.base.time.format("%H:%M:%S%.3f"))?;
					let mut len = 13;
					if p.base.incoming {
						let text = style("IN  ").with(IN_COLOR);
						#[cfg(unix)]
						let text = text.bold();
						text.paint(screen);
					} else {
						let text = style("OUT ").with(OUT_COLOR);
						#[cfg(unix)]
						let text = text.bold();
						text.paint(screen);
					}
					color.set_bg(BACKGROUND_COLOR, screen);
					len += 4;

					// Content
					write!(screen, "{}", cmd.command)?;
					len += cmd.command.width();

					// Fill the rest with background color
					let len = len % width as usize;
					if width as usize > len {
						write!(screen, "{}\r\n", " ".repeat(width as usize - len))?;
					}
				}
				_ => {}
			}
		}

		Ok(())
	}

	/// Return the position of the cursor.
	fn draw_input(input: &str, cursor_pos: usize, width: u16, height: u16, screen: &mut Screen,
		cursor: &TerminalCursor, color: &mut TerminalColor) -> Result<(u16, u16)> {

		// TODO Compute text height
		let h = 1;

		cursor.goto(0, height - 1 - h, screen);

		// Separator
		color.set_bg(SEPARATOR_COLOR, screen);
		write!(screen, "{}\r\n", " ".repeat(width as usize))?;
		color.set_bg(BACKGROUND_COLOR, screen);

		write!(screen, "> {}", input)?;
		let len = 2 + input.width();

		// Fill the rest with background color
		if width as usize > len {
			write!(screen, "{}", " ".repeat(width as usize - len))?;
		}

		Ok((2 + cursor_pos as u16, height - 1))
	}

	fn draw(&mut self) -> Result<()> {
		let screen = &mut self.alt_screen.screen;
		let term = self.cross.terminal();
		let cursor = self.cross.cursor();
		let mut color = self.cross.color();

		cursor.save_position(screen);
		if self.table_header_invalid && self.table_invalid
			&& self.details_invalid && self.input_invalid {
			// TODO Fill everything with background color
			term.clear(ClearType::All, screen);
		}
		cursor.hide(screen);
		color.set_bg(BACKGROUND_COLOR, screen);

		if self.width < 10 || self.height < 5 {
			cursor.goto(0, 0, screen);
			color.set_fg(ALERT_COLOR, screen);
			write!(screen, "Terminal too small")?;
			color.reset(screen);

			cursor.reset_position(screen);
			cursor.show(screen);
			screen.flush_buf()?;
			return Ok(());
		}

		if self.table_header_invalid {
			self.table_header_invalid = false;
			Self::draw_table_header(self.width, screen, &cursor, &mut color)?;
		}
		color.set_fg(FONT_COLOR, screen);
		if self.table_invalid {
			self.table_invalid = false;
			self.table.paint(Rect::new(0, 1, self.width, self.height - 3),
				screen, &cursor, &mut color)?;
		}
		if self.details_invalid {
			self.details_invalid = false;
			//Self::draw_details(self.width, screen, &cursor, &mut color)?;
		}

		let cursor_pos = if self.input_invalid {
			self.input_invalid = false;
			Some(Self::draw_input(&self.input, self.cursor_pos, self.width,
				self.height, screen, &cursor, &mut color)?)
		} else {
			None
		};

		color.reset(screen);

		if let Some((x, y)) = cursor_pos {
			cursor.goto(x, y, screen);
		} else {
			cursor.reset_position(screen);
		}
		cursor.show(screen);
		screen.flush_buf()?;
		Ok(())
	}

	fn key_input(&mut self, k: Key) {
		// Use rustyline with custom terminal
		let prev_invalid = self.input_invalid;
		self.input_invalid = true;
		match k {
			Key::Char(c) => {
				self.input.insert(self.cursor_pos, c);
				self.cursor_pos += 1;
			}
			Key::Right => if self.cursor_pos < self.input.len() {
				self.cursor_pos += 1;
			}
			Key::Left => if self.cursor_pos > 0 {
				self.cursor_pos -= 1;
			}
			Key::Home => self.cursor_pos = 0,
			Key::End => self.cursor_pos = self.input.len(),
			Key::Backspace => if self.cursor_pos > 0 {
				self.input.remove(self.cursor_pos - 1);
				self.cursor_pos -= 1;
			}
			Key::Enter => {
				match Command::read((), &mut std::io::Cursor::new(&self.input)) {
					Ok(cmd) => {
						self.cursor_pos = 0;
						self.input.clear();
						STATE.lock().unwrap().command_sender.unbounded_send(Some(cmd)).unwrap();
					}
					Err(_) => {
						// TODO Show error
					}
				}
			}
			Key::Delete => if self.input.len() > self.cursor_pos {
				self.input.remove(self.cursor_pos);
			}
			_ => self.input_invalid = prev_invalid,
		}
	}

	fn mouse_input(&mut self, m: MouseEvent) {
		match m {
			MouseEvent { input: MouseInput::WheelUp, position, .. }
			| MouseEvent { input: MouseInput::WheelDown, position, .. } => {
				// Check if the mouse is hovering the table
				if self.height < 3 || position.line == 0
					|| position.line >= self.height as usize - 3 {
					return;
				}

				let lines = if m.input == MouseInput::WheelDown {
					SCROLL_LINES
				} else {
					-SCROLL_LINES
				};
				self.table_invalid |= self.table.scroll(lines);
			}
			_ => {}
		}
	}

	pub fn event_loop(&mut self) -> Result<()> {
		let recv = STATE.lock().unwrap().ui_recv.take().unwrap();
		let (width, height) = self.cross.terminal().terminal_size();
		self.table.set_size(width, height - 3);
		self.draw()?;
		// TODO Accumulate events
		while let Ok(e) = recv.recv() {
			match e {
				UiEvent::Redraw => self.draw()?,
				UiEvent::Resize => {
					let (width, height) = self.cross.terminal().terminal_size();
					if self.width != width || self.height != height {
						self.width = width;
						self.height = height;
						self.table_header_invalid = true;
						self.table_invalid = true;
						self.details_invalid = true;
						self.input_invalid = true;

						self.table.set_size(width, height - 3);
						self.table.update_entries();

						self.draw()?;
					}
				}
				UiEvent::KeyEvent(k) => {
					self.key_input(k);
					if self.input_invalid {
						self.draw()?;
					}
				}
				UiEvent::MouseEvent(m) => {
					self.mouse_input(m);
					if self.table_header_invalid || self.table_invalid
						|| self.details_invalid || self.input_invalid {
						self.draw()?;
					}
				}
				UiEvent::Log => {
					if self.table.visibility.log && self.table.needs_update() {
						self.table.update_entries();
						self.table_invalid = true;
						self.draw()?;
					}
				}
				UiEvent::Message => {
					if self.table.visibility.messages && self.table.needs_update() {
						self.table.update_entries();
						self.table_invalid = true;
						self.draw()?;
					}
				}
				UiEvent::Packet => {
					if self.table.visibility.packets && self.table.needs_update() {
						self.table.update_entries();
						self.table_invalid = true;
						self.draw()?;
					}
				}
				UiEvent::UdpPacket => {
					if self.table.visibility.udp_packets && self.table.needs_update() {
						self.table.update_entries();
						self.table_invalid = true;
						self.draw()?;
					}
				}
				UiEvent::Quit => break,
			}
		}
		Ok(())
	}
}

pub fn ui() -> Result<()> {
	let term = mortal::Terminal::new()?;
	let prepare_config = mortal::PrepareConfig {
		enable_mouse: true,
		block_signals: false,
		report_signals: [Signal::Interrupt, Signal::Resize].iter().cloned().collect(),
		.. mortal::PrepareConfig::default()
	};

	let mut ui = Ui::new()?;
	let ui_thread = std::thread::spawn(move || if let Err(e) = ui.event_loop() {
		eprintln!("Error drawing tui: {:?}", e);
	});

	let mut read = term.lock_read().unwrap();
	let state = read.prepare(prepare_config)?;
	let r = inner_ui(&mut read);

	// Drop the crossterm terminal before mortal so removing the alternate
	// screen works.
	if let Err(e) = ui_thread.join() {
		eprintln!("Ui thread panicked: {:?}", e.downcast_ref::<String>());
	}
	read.restore(state)?;
	let _ = STATE.lock().unwrap().command_sender.unbounded_send(None);
	r
}

fn inner_ui(read: &mut mortal::terminal::TerminalReadGuard) -> Result<()> {
	loop {
		// Wait for input with mortal
		if let Some(event) = read.read_event(None)? {
			match event {
				Event::Key(event) => {
					STATE.lock().unwrap().ui_sender.send(
						UiEvent::KeyEvent(event))?;
				}
				Event::Mouse(event) => {
					STATE.lock().unwrap().ui_sender.send(
						UiEvent::MouseEvent(event))?;
				}
				Event::Resize(_) =>
					STATE.lock().unwrap().ui_sender.send(UiEvent::Resize)?,
				Event::Signal(Signal::Interrupt) => {
					break;
				}
				_ => {}
			}
		}
	}
	STATE.lock().unwrap().ui_sender.send(UiEvent::Quit)?;
	Ok(())
}

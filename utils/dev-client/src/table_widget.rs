use std::io::Write;

use chrono::{Date, Utc};
use crossterm::{Screen, TerminalColor, TerminalCursor};
use crossterm::style::style;
use tsproto::commands::Command;

use crate::{LogRecord, MessageRecord, Result, State, STATE};
use crate::colors::*;
use crate::text_widget::{Align, TextWidget};
use crate::ui::{Rect, Widget};

pub struct TableWidget {
	width: u16,
	height: u16,
	entries: Vec<EntryWidget>,

	pub visibility: EntryVisibility,

	/// The line offset of the first displayed element.
	first_scroll_offset: u16,
	/// If the table follows the output, so the latest entry is always displayed
	/// at the bottom.
	following: bool,

	position: EntryPosition,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EntryVisibility {
	pub log: bool,
	pub messages: bool,
	pub packets: bool,
	pub udp_packets: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct EntryPosition(usize);

#[derive(Clone)]
pub(crate) enum Entry {
	NewDate(Date<Utc>),
	Log(LogRecord, TextWidget),
	Message(MessageRecord, TextWidget),
}

/// One entry in the table.
#[derive(Clone)]
pub(crate) struct EntryWidget(pub(crate) Entry);

const ENTRY_HEADER_LEN: u16 = 17;
const LOG_HEADER_LEN: u16 = ENTRY_HEADER_LEN - 3;

impl EntryPosition {
	fn get_entry(self, vis: &EntryVisibility, state: &State)
		-> Option<EntryWidget> {
		if let Some(e) = state.entries.get(self.0) {
			if match e.0 {
				Entry::Log(_, _) => vis.log,
				Entry::Message(_, _) => vis.messages,
				_ => true,
			} {
				return Some(e.clone());
			}
		}

		None
	}

	fn next(self, vis: &EntryVisibility, state: &State) -> Option<EntryPosition> {
		let mut i = self.0 + 1;
		let mut is_none = true;
		while i < state.entries.len() && {
			is_none = EntryPosition(i).get_entry(vis, state).is_none();
			is_none
		} {
			i += 1;
		}

		if is_none { None } else { Some(EntryPosition(i)) }
	}

	fn prev(self, vis: &EntryVisibility, state: &State) -> Option<EntryPosition> {
		if self.0 == 0 {
			return None;
		}
		let mut i = self.0 - 1;
		let mut is_none;
		while {
			is_none = EntryPosition(i).get_entry(vis, state).is_none();
			is_none
		} && i > 0 {
			i -= 1;
		}

		if is_none { None } else { Some(EntryPosition(i)) }
	}
}

impl Entry {
	pub(crate) fn new_log(rec: LogRecord) -> Self {
		let mut t = TextWidget::new();
		t.content = rec.data.clone();
		t.update_length();
		Entry::Log(rec, t)
	}

	pub(crate) fn new_message(rec: MessageRecord) -> Self {
		let cmd: Command = (&rec.data).into();
		let mut t = TextWidget::new();
		t.content.push(style(cmd.command).on(BACKGROUND_COLOR).no_reset());
		t.update_length();
		Entry::Message(rec, t)
	}

	pub(crate) fn get_date(&self) -> Date<Utc> {
		match self {
			Entry::NewDate(d) => *d,
			Entry::Log(e, _) => e.time.date(),
			Entry::Message(e, _) => e.time.date(),
		}
	}
}

impl EntryWidget {
	fn set_align(&mut self, align: Align) {
		match &mut self.0 {
			Entry::NewDate(_) => {}
			Entry::Log(_, t) |
			Entry::Message(_, t) => t.vertical_align = align,
		}
	}
}

impl Widget for EntryWidget {
	fn set_size(&mut self, width: u16, height: u16) {
		if width <= ENTRY_HEADER_LEN {
			return;
		}
		match &mut self.0 {
			Entry::NewDate(_) => {}
			Entry::Log(_, t) => t.set_size(width - LOG_HEADER_LEN, height),
			Entry::Message(_, t) => t.set_size(width - ENTRY_HEADER_LEN, height),
		}
	}

	fn width(&self) -> u16 {
		match &self.0 {
			Entry::NewDate(_) => 10,
			Entry::Log(_, t) => LOG_HEADER_LEN + t.width(),
			Entry::Message(_, t) => ENTRY_HEADER_LEN + t.width(),
		}
	}

	fn height(&self) -> u16 {
		match &self.0 {
			Entry::NewDate(_) => 1,
			Entry::Log(_, t) |
			Entry::Message(_, t) => t.height(),
		}
	}

	fn paint(&self, mut area: Rect, screen: &mut Screen, cursor: &TerminalCursor,
		color: &mut TerminalColor) -> Result<()> {
		if area.width <= ENTRY_HEADER_LEN {
			return Ok(());
		}
		cursor.goto(area.x, area.y, screen);

		match &self.0 {
			Entry::NewDate(d) => {
				write!(screen, "{}", d.format("%Y-%m-%d"))?;
			}
			Entry::Log(r, t) => {
				write!(screen, "{}", r.time.format("%H:%M:%S%.3f"))?;

				// Content
				area.width -= LOG_HEADER_LEN;
				area.x += LOG_HEADER_LEN;
				t.paint(area, screen, cursor, color)?;
			}
			Entry::Message(p, t) => {
				write!(screen, "{} ", p.base.time.format("%H:%M:%S%.3f"))?;
				if p.base.incoming {
					let text = style("IN").with(IN_COLOR);
					text.paint(screen);
				} else {
					let text = style("OUT").with(OUT_COLOR);
					text.paint(screen);
				}
				color.set_bg(BACKGROUND_COLOR, screen);
				color.set_fg(FONT_COLOR, screen);

				// Content
				area.width -= ENTRY_HEADER_LEN;
				area.x += ENTRY_HEADER_LEN;
				t.paint(area, screen, cursor, color)?;
			}
		}

		Ok(())
	}
}

impl TableWidget {
	pub fn new() -> Self {
		Self {
			width: 1,
			height: 1,
			entries: Default::default(),

			visibility: EntryVisibility {
				log: true,
				messages: true,
				packets: false,
				udp_packets: false,
			},

			first_scroll_offset: 0,
			following: true,

			position: EntryPosition(0),
		}
	}

	/// Update the list of currently listed/cached entries.
	///
	/// This is affected by scrolling, changing the size and new entries.
	pub fn update_entries(&mut self) {
		let state = STATE.lock().unwrap();
		let state = &*state;
		self.entries.clear();
		if self.following {
			// Start to fill from the bottom
			let mut entry = self.position;
			while let Some(e) = entry.next(&self.visibility, state) {
				entry = e;
			}
			self.position = entry;

			let mut height = self.height;
			let mut entry = self.position;
			while let Some(mut w) = entry.get_entry(&self.visibility, state) {
				w.set_size(self.width, height);
				let h = w.height();
				self.entries.push(w);
				if h >= height {
					self.first_scroll_offset = h - height;
					break;
				}
				height -= h;

				entry = match entry.prev(&self.visibility, state) {
					Some(e) => e,
					None => break,
				};
			}

			self.entries.reverse();
		} else {
			let mut pos = self.position;
			let mut height = self.height + self.first_scroll_offset;
			while let Some(p) = pos.next(&self.visibility, state) {
				let mut w = p.get_entry(&self.visibility, state).unwrap();
				w.set_size(self.width, height);
				let h = w.height();
				self.entries.push(w);
				if h >= height {
					break;
				}
				height -= h;
				pos = p;
			}
		}

		if let Some(first) = self.entries.first_mut() {
			first.set_align(Align::Bottom);
		}
	}

	/// Scroll down a specified amount of lines.
	///
	/// If `lines` is negative, it scrolls up.
	///
	/// Returns if something changed and this widget has to be updated.
	pub fn scroll(&mut self, mut lines: isize) -> bool {
		let state_lock = STATE.lock().unwrap();
		let state = &*state_lock;
		if lines < 0 {
			lines = -lines;

			// Correct current position
			if self.following {
				if self.entries.len() >= self.position.0 {
					self.position = EntryPosition(0);
				} else {
					self.position = EntryPosition(
						self.position.0 - self.entries.len());
				}
			}

			if self.first_scroll_offset as isize >= lines {
				self.following = false;
				self.first_scroll_offset -= lines as u16;
				drop(state_lock);
				self.update_entries();
				return true;
			}

			lines -= self.first_scroll_offset as isize;
			if self.first_scroll_offset > 0 {
				self.following = false;
			}
			let mut pos = self.position;
			while let Some(p) = pos.prev(&self.visibility, state) {
				pos = p;
				let mut w = p.get_entry(&self.visibility, state).unwrap();
				w.set_size(self.width, self.height);
				let h = w.height();
				if h as isize >= lines {
					self.first_scroll_offset = h - lines as u16;
					break;
				}
				lines -= h as isize;
			}

			if self.position != pos {
				self.following = false;
				self.position = pos;
				drop(state_lock);
				self.update_entries();
			}
			true
		} else if lines > 0 && !self.following {
			let heights: Vec<_> = self.entries.iter()
				.map(|e| e.height() as usize).collect();
			let sum = heights.iter().sum::<usize>();
			if sum < self.height as usize {
				// TODO This should not happen
				self.following = true;
				return false;
			}
			let diff = sum - self.height as usize;

			// TODO compute first_scroll_offset

			// If there is more room to scroll down
			let has_next;
			if lines as usize > diff {
				lines -= diff as isize;
				// Compute current end
				let mut pos = EntryPosition(self.position.0 + heights.len() - 1);

				let mut add_count = 0;
				while lines > 0 {
					// Add element at the bottom
					if let Some(p) = pos.next(&self.visibility, state) {
						let mut w = p.get_entry(&self.visibility, state).unwrap();
						w.set_size(self.width, self.height);
						let h = w.height() as isize;
						add_count += 1;
						pos = p;
						if h >= lines {
							break;
						}
						lines -= h;
					} else {
						break;
					}
				}
				has_next = pos.next(&self.visibility, state).is_some();
				self.position = EntryPosition(self.position.0 + add_count);
			} else {
				let pos = EntryPosition(self.position.0 + heights.len() - 1);
				has_next = pos.next(&self.visibility, state).is_some();
			}

			// Still not full
			if !has_next {
				self.following = true;
			}
			drop(state_lock);
			self.update_entries();
			true
		} else {
			false
		}
	}

	/// If the table should be updated when a new entry is added at the bottom.
	///
	/// The table should be updated if it is `following` or if there is space
	/// left to show more entries.
	pub fn needs_update(&self) -> bool {
		self.following || {
			let sum = self.entries.iter()
				.map(|e| e.height() as usize).sum::<usize>();
			sum < self.height as usize
		}
	}
}

impl Widget for TableWidget {
	fn set_size(&mut self, width: u16, mut height: u16) {
		self.width = width;
		self.height = height;
		for e in &mut self.entries {
			e.set_size(width, height);
			let h = e.height();
			if height <= h {
				break;
			}
			height -= h;
		}
		self.update_entries();
	}

	fn width(&self) -> u16 {
		25
	}
	fn height(&self) -> u16 {
		5
	}

	fn paint(&self, mut area: Rect, screen: &mut Screen, cursor: &TerminalCursor,
		color: &mut TerminalColor) -> Result<()> {
		color.set_fg(FONT_COLOR, screen);

		// Clear background
		cursor.goto(area.x, area.y, screen);
		write!(screen, "{}", format!("{}\r\n", " ".repeat(area.width as usize))
			.repeat(area.height as usize))?;

		for e in &self.entries {
			let h = e.height();
			e.paint(area.clone(), screen, cursor, color)?;
			if area.height <= h {
				break;
			}
			area.y += h;
			area.height -= h;
		}

		Ok(())
	}
}

use std::io::Write;

use crossterm::{Screen, StyledObject, TerminalColor, TerminalCursor};
use unicode_width::UnicodeWidthStr;
use unicode_segmentation::UnicodeSegmentation;

use crate::Result;
use crate::ui::{Rect, Widget};

#[derive(Eq, Debug, Clone, Copy, PartialEq)]
pub enum Align {
	Top,
	Bottom,
}

#[derive(Clone)]
pub struct TextWidget {
	width: u16,
	height: u16,
	/// The length of the content
	len: usize,
	/// Call `update_length()` after changing the content!
	pub content: Vec<StyledObject<String>>,
	pub vertical_align: Align,
}

impl TextWidget {
	pub fn new() -> Self {
		Self {
			width: 0,
			height: 0,
			len: 0,
			content: Default::default(),
			vertical_align: Align::Top,
		}
	}

	/// Compute the length of the content.
	///
	/// This needs to be called after every change to the `content` to get
	/// accurate height and width requirements.
	pub fn update_length(&mut self) {
		self.len = self.content.iter().map(|s| s.content.width()).sum();
	}
}

impl Widget for TextWidget {
	fn set_size(&mut self, width: u16, height: u16) {
		self.width = width;
		self.height = height;
	}

	fn width(&self) -> u16 {
		if self.height == 0 {
			return 0;
		}
		((self.len + self.height as usize - 1) / self.height as usize) as u16
	}
	fn height(&self) -> u16 {
		if self.width == 0 {
			return 0;
		}
		((self.len + self.width as usize - 1) / self.width as usize) as u16
	}

	fn paint(&self, area: Rect, screen: &mut Screen, cursor: &TerminalCursor,
		color: &mut TerminalColor) -> Result<()> {
		let mut old_x = 0;
		let mut x = 0;
		let mut y = 0;
		let wanted_height = self.height();
		let start_y = if wanted_height < area.height && self.vertical_align == Align::Bottom {
			area.height - wanted_height
		} else {
			0
		};

		for c in &self.content {
			// Set style
			let o = StyledObject { object_style: c.object_style.clone(), content: "", reset: false };
			o.paint(screen);
			let mut pos = 0;
			for (i, s) in c.content.grapheme_indices(true) {
				let w = s.width();
				if x + w > area.width as usize {
					if y >= start_y {
						cursor.goto(area.x + old_x, area.y + y, screen);
						write!(screen, "{}", &c.content[pos..i])?;
					}
					pos = i;
					x = w;
					old_x = 0;
					y += 1;
					if y > area.height {
						return Ok(());
					}
				} else {
					x += w;
				}
			}
			cursor.goto(area.x + old_x, area.y + y, screen);
			write!(screen, "{}", &c.content[pos..])?;
			old_x = x as u16;

			// Reset color if necessary
			if c.reset && (c.object_style.bg_color.is_some()
				|| c.object_style.fg_color.is_some()) {
				color.reset(screen);
			}

			#[cfg(unix)] {
				if c.reset && !c.object_style.attrs.is_empty() {
					color.reset(screen);
				}
			}
		}

		Ok(())
	}
}

use crossterm::Color;

// TODO Configure in struct/toml
#[cfg(unix)]
pub const BACKGROUND_COLOR: Color = Color::Rgb{r:0x26,g:0x26,b:0x26};
#[cfg(not(unix))]
pub const BACKGROUND_COLOR: Color = Color::Black;
pub const FONT_COLOR: Color = Color::White;
#[cfg(unix)]
pub const HEADER_COLOR: Color = Color::Rgb{r:0x8b,g:0x91,b:0x97};
#[cfg(not(unix))]
pub const HEADER_COLOR: Color = Color::Grey;
#[cfg(unix)]
pub const SEPARATOR_COLOR: Color = Color::Rgb{r:0x1c,g:0x1c,b:0x1c};
#[cfg(not(unix))]
pub const SEPARATOR_COLOR: Color = Color::Black;
pub const ALERT_COLOR: Color = Color::Red;

pub const IN_COLOR: Color = Color::Green;
pub const OUT_COLOR: Color = Color::Red;

pub const CRIT_COLOR: Color = Color::DarkRed;
pub const ERRO_COLOR: Color = Color::Red;
pub const WARN_COLOR: Color = Color::Yellow;
pub const INFO_COLOR: Color = Color::Green;
pub const DEBG_COLOR: Color = Color::Cyan;
pub const TRAC_COLOR: Color = Color::White;

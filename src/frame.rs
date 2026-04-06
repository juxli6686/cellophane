use std::{fmt::{self, Display}, io};

use crossterm::style::{Color, Stylize};
use bitflags::bitflags;
use smallvec::SmallVec;
use unicode_segmentation::UnicodeSegmentation;
use unicode_width::UnicodeWidthChar;

const ZWJ: char = '\u{200D}';

/// A rectangular grid of [`Cell`]s representing a single frame of terminal output.
///
/// All rows are guaranteed to have equal length. Use [`with_capacity`](Frame::with_capacity)
/// or [`from_cells`](Frame::from_cells) to construct, and [`get_cell`](Frame::get_cell) /
/// [`get_cell_mut`](Frame::get_cell_mut) to access individual cells.
#[derive(Default,Clone,Debug,Hash)]
pub struct Frame(Vec<Vec<Cell>>);

type Rows = usize;
type Cols = usize;
impl Frame {
	/// Take ownership of the frame's contents, replacing them with a blank grid of the same dimensions.
	/// Useful for extracting the current frame without cloning when you only have a mutable borrow.
	pub fn take(&mut self) -> Self {
		let (rows,cols) = self.dims().unwrap_or((0,0));
		let new_cells = Self::with_capacity(cols, rows).0;
		Frame(std::mem::replace(&mut self.0, new_cells))
	}
	/// Consume the frame and return the underlying cell grid.
	/// Use [`from_cells`](Frame::from_cells) to reconstruct a `Frame` from the result.
	pub fn into_cells(self) -> Vec<Vec<Cell>> {
		self.0
	}
	/// Borrow the underlying cell grid.
	pub fn cells(&self) -> &Vec<Vec<Cell>> {
		&self.0
	}
	/// This is pub(crate) because giving direct access to the cell vector
	/// would allow consumers to break the 'all rows are the same length' invariant.
	pub(crate) fn cells_mut(&mut self) -> &mut Vec<Vec<Cell>> {
		&mut self.0
	}
	/// Returns a reference to the cell at the given row and column, or `None` if out of bounds.
	pub fn get_cell(&self, row: usize, col: usize) -> Option<&Cell> {
		self.0.get(row).and_then(|r| r.get(col))
	}
	/// Returns a mutable reference to the cell at the given row and column, or `None` if out of bounds.
	pub fn get_cell_mut(&mut self, row: usize, col: usize) -> Option<&mut Cell> {
		self.0.get_mut(row).and_then(|r| r.get_mut(col))
	}
	/// Create a blank frame with the given dimensions, filled with default (empty) cells.
	pub fn with_capacity(cols: usize, rows: usize) -> Self {
		Frame(vec![vec![Cell::default(); cols]; rows])
	}
	/// Construct a frame from a pre-built cell grid.
	///
	/// # Panics
	/// Panics if the rows have different lengths.
	pub fn from_cells(cells: Vec<Vec<Cell>>) -> Self {
		let len = cells.first().map(|row| row.len());
		if let Some(len) = len {
			assert!(
				cells.iter().all(|row| row.len() == len),
				"all rows in a Frame must have equal length",
			)
		}
		Frame(cells)
	}
	/// Create a blank frame matching the current terminal size.
	/// Falls back to 80x24 if the terminal size cannot be determined.
	pub fn from_terminal() -> Self {
		let (cols,rows) = crossterm::terminal::size().unwrap_or((80, 24));
		let mut builder = FrameBuilder::new(cols as usize, rows as usize);
		builder.feed_bytes(b"\x1b[?25l"); // hide cursor
		builder.feed_bytes(b"\x1b[2J"); // clear screen
		builder.feed_bytes(b"\x1b[H"); // move cursor to top-left
		let mut frame = builder.build();
		frame.resize(cols as usize, rows as usize);
		frame
	}
	/// Run a command and parse its stdout (including ANSI escape codes) into a frame.
	/// The `COLUMNS` environment variable is set to the current terminal width.
	pub fn from_command(mut command: std::process::Command) -> io::Result<Self> {
		let (cols,rows) = crossterm::terminal::size().unwrap_or((80, 24));
		let output = command
			.env("COLUMNS", cols.to_string())
			.output()?;

		let mut builder = FrameBuilder::new(cols as usize, rows as usize);
		builder.feed_bytes(&output.stdout);
		let mut frame = builder.build();
		frame.resize(cols as usize, rows as usize);
		Ok(frame)
	}
	/// Returns the dimensions as `(rows, cols)`, or `None` if the frame is empty.
	pub fn dims(&self) -> Option<(Rows,Cols)> {
		let rows = self.0.len();
		if rows == 0 {
			return None;
		}
		let cols = self.0[0].len();
		Some((rows, cols))
	}

	/// Resize the frame to the given width and height.
	/// New cells are filled with defaults; excess cells are truncated.
	pub fn resize(&mut self, w: usize, h: usize) {
		// adjust columns on existing rows
		for row in &mut self.0 {
			row.resize(w, Cell::default());
		}
		// adjust row count
		self.0.resize(h, vec![Cell::default(); w]);
	}
}

/// Parses ANSI-escaped text into a [`Frame`] using a VTE state machine.
///
/// Supports SGR attributes (bold, italic, colors, etc.), 256-color and 24-bit RGB,
/// ZWJ emoji sequences, and wide characters.
///
/// # Example
/// ```
/// use cellophane::FrameBuilder;
///
/// let mut builder = FrameBuilder::new(80, 24);
/// builder.feed_str("Hello, \x1b[1;31mworld\x1b[0m!");
/// let frame = builder.build();
/// ```
pub struct FrameBuilder {
	frame: Frame,
	row: usize,
	rows: usize,
	col: usize,
	cols: usize,
	last_pos: Option<(usize,usize)>,
	pending_zwj: bool,
	current_fg: Color,
	current_bg: Color,
	current_flags: CellFlags,
	parser: vte::Parser
}

impl FrameBuilder {
	/// Create a new builder with the given grid dimensions.
	pub fn new(cols: usize, rows: usize) -> Self {
		Self {
			frame: Frame::from_cells(vec![vec![Cell::default(); cols]; rows]),
			row: 0,
			rows,
			col: 0,
			cols,
			last_pos: None,
			pending_zwj: false,
			current_fg: Color::Reset,
			current_bg: Color::Reset,
			current_flags: CellFlags::empty(),
			parser: vte::Parser::new()
		}
	}
	/// Feed raw bytes into the parser. Useful for piping command output directly.
	pub fn feed_bytes(&mut self, bytes: &[u8]) {
		let mut parser = std::mem::take(&mut self.parser);
		parser.advance(self, bytes);
		self.parser = parser;
	}
	/// Feed a string into the parser. Convenience wrapper around [`feed_bytes`](FrameBuilder::feed_bytes).
	pub fn feed_str(&mut self, s: &str) {
		self.feed_bytes(s.as_bytes());
	}
	/// Consume the builder and return the constructed [`Frame`].
	pub fn build(self) -> Frame {
		self.frame
	}
}

impl vte::Perform for FrameBuilder {
	fn print(&mut self, c: char) {
		// handle zero-width joiners
		if (c == ZWJ || self.pending_zwj)
		&& let Some((row,col)) = self.last_pos
		&& let Some(last_cell) = self.frame.get_cell_mut(row, col) {
			last_cell.push_char(c);
			self.pending_zwj = c == ZWJ;
			return;
		}
		self.pending_zwj = false;

		if self.col >= self.cols {
			self.col = 0;
			self.row += 1;
		}
		if self.row >= self.rows {
			self.frame.cells_mut().push(vec![Cell::default(); self.cols]);
			self.rows += 1;
		}
	  let cell = Cell::new(c, self.current_fg, self.current_bg, self.current_flags);
		let Some(frame_cell) = self.frame.get_cell_mut(self.row, self.col) else { return };
		*frame_cell = cell;
		self.last_pos = Some((self.row, self.col));
		let width = UnicodeWidthChar::width(c).unwrap_or(1);
		if width > 1
		&& let Some(next) = self.frame.get_cell_mut(self.row, self.col + 1) {
			next.flags |= CellFlags::WIDE_CONTINUATION;
		}
		self.col += width;
	}
	fn execute(&mut self, byte: u8) {
		match byte {
			b'\n' => {
				self.row += 1;
				self.col = 0;
				if self.row >= self.rows {
					self.frame.cells_mut().push(vec![Cell::default(); self.cols]);
					self.rows += 1;
				}
			}
			b'\r' => {
				self.col = 0;
			}
			_ => {}
		}
	}
	fn csi_dispatch(
		&mut self,
		params: &vte::Params,
		_intermediates: &[u8],
		_ignore: bool,
		action: char,
	) {
		if action != 'm' { return; }
		let params: Vec<u16> = params.iter()
			.flat_map(|p| p.iter().copied())
			.collect();

		let mut i = 0;
		while i < params.len() {
			let Some(param) = params.get(i) else { continue; };
			match param {
				0 => {
					self.current_fg = Color::Reset;
					self.current_bg = Color::Reset;
					self.current_flags = CellFlags::empty();
				}
				1 => self.current_flags.insert(CellFlags::BOLD),
				2 => self.current_flags.insert(CellFlags::DIM),
				3 => self.current_flags.insert(CellFlags::ITALIC),
				4 => self.current_flags.insert(CellFlags::UNDERLINE),
				7 => self.current_flags.insert(CellFlags::INVERSE),
				8 => self.current_flags.insert(CellFlags::HIDDEN),
				9 => self.current_flags.insert(CellFlags::STRIKETHROUGH),
				30..=37 => self.current_fg = Color::AnsiValue((params[i] - 30) as u8),
				38 | 48 => {
					let is_bg = *param == 48;
					i += 1;
					let Some(param2) = params.get(i) else { continue; };
					match param2 {
						5 => {
							i += 1;
							let Some(param3) = params.get(i) else { continue; };
							let color = Color::AnsiValue(*param3 as u8);
							if is_bg {
								self.current_bg = color;
							} else {
								self.current_fg = color;
							}
						}
						2 => {
							i += 1;
							let Some(param3) = params.get(i) else { continue; };
							i += 1;
							let Some(param4) = params.get(i) else { continue; };
							i += 1;
							let Some(param5) = params.get(i) else { continue; };

							let color = Color::Rgb { r: *param3 as u8, g: *param4 as u8, b: *param5 as u8 };
							if is_bg {
								self.current_bg = color;
							} else {
								self.current_fg = color;
							}
						}
						_ => {}
					}
				}
				39 => self.current_fg = Color::Reset,
				40..=47 => self.current_bg = Color::AnsiValue((params[i] - 40) as u8),
				49 => self.current_bg = Color::Reset,
				90..=97 => self.current_fg = Color::AnsiValue((params[i] - 90 + 8) as u8),
				100..=107 => self.current_bg = Color::AnsiValue((params[i] - 100 + 8) as u8),
				_ => { /* ignore unknown params */ }
			}
			i += 1;
		}
	}
}

/// A unicode grapheme cluster backed by `SmallVec<[char; 4]>`.
///
/// Most graphemes fit in 1–4 codepoints (ASCII, accented characters, ZWJ emoji)
/// and stay stack-allocated. Graphemes exceeding 4 codepoints gracefully spill to the heap.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Grapheme(SmallVec<[char; 4]>);

impl Grapheme {
  /// Returns the constituent chars of this grapheme as a slice.
  pub fn chars(&self) -> &[char] {
    &self.0
  }
  /// Returns the display width of the grapheme, treating unprintable chars as width 0.
  pub fn width(&self) -> usize {
    self.0.iter().map(|c| c.width().unwrap_or(0)).sum()
  }
  /// Returns `true` if this grapheme is a linefeed (`'\n'`).
  pub fn is_lf(&self) -> bool {
    self.is_char('\n')
  }
  /// Returns `true` if this grapheme consists of exactly one char equal to `c`.
  pub fn is_char(&self, c: char) -> bool {
    self.0.len() == 1 && self.0[0] == c
  }
  /// If this grapheme is a single char, returns it. Otherwise returns `None`.
  pub fn as_char(&self) -> Option<char> {
    if self.0.len() == 1 {
      Some(self.0[0])
    } else {
      None
    }
  }

	/// Append a codepoint to this grapheme. Used internally to build up
	/// multi-codepoint sequences like ZWJ emoji.
	pub fn push_char(&mut self, c: char) {
		self.0.push(c);
	}

	/// Returns `true` if all codepoints in this grapheme are whitespace.
	pub fn is_whitespace(&self) -> bool {
		self.0.iter().all(|c| c.is_whitespace())
	}
}

impl From<char> for Grapheme {
  fn from(value: char) -> Self {
    let mut new = SmallVec::<[char; 4]>::new();
    new.push(value);
    Self(new)
  }
}

impl From<&str> for Grapheme {
  fn from(value: &str) -> Self {
    assert_eq!(value.graphemes(true).count(), 1);
    let mut new = SmallVec::<[char; 4]>::new();
    for char in value.chars() {
      new.push(char);
    }
    Self(new)
  }
}

impl From<String> for Grapheme {
  fn from(value: String) -> Self {
    Into::<Self>::into(value.as_str())
  }
}

impl From<&String> for Grapheme {
  fn from(value: &String) -> Self {
    Into::<Self>::into(value.as_str())
  }
}

impl Display for Grapheme {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    for ch in &self.0 {
      write!(f, "{ch}")?;
    }
    Ok(())
  }
}

/// Split a string into a `Vec` of [`Grapheme`]s using Unicode segmentation.
/// Each element represents a single user-perceived character, which may consist of multiple codepoints.
pub fn to_graphemes(s: impl ToString) -> Vec<Grapheme> {
  let s = s.to_string();
  s.graphemes(true).map(Grapheme::from).collect()
}
bitflags! {
	#[derive(Default,Clone,Copy,Debug,PartialEq,Eq,Hash)]
	/// Bitflags representing text attributes for a Cell. These correspond to the common ANSI SGR attributes.
	pub struct CellFlags: u32 {
		const BOLD              = 0b000000001;
		const ITALIC            = 0b000000010;
		const UNDERLINE         = 0b000000100;
		const INVERSE           = 0b000001000;
		const HIDDEN            = 0b000010000;
		const STRIKETHROUGH     = 0b000100000;
		const DIM               = 0b001000000;
		const BLINK             = 0b010000000;
		const WIDE_CONTINUATION = 0b100000000;
	}
}

/// A single terminal cell with a character, foreground/background colors, and text attributes.
///
/// The default cell is a space with no colors or attributes set.
/// Cells can be constructed with the builder pattern ([`with_char`](Cell::with_char),
/// [`with_fg`](Cell::with_fg), etc.) or mutated in place ([`set_char`](Cell::set_char),
/// [`set_fg`](Cell::set_fg), etc.).
#[derive(Clone,Debug,PartialEq,Eq,Hash)]
pub struct Cell {
	ch: Grapheme,
	fg: Color,
	bg: Color,
	flags: CellFlags
}

impl Default for Cell {
	fn default() -> Self {
		Self::new(' ', Color::Reset, Color::Reset, CellFlags::empty())
	}
}

impl Cell {
	/// Create a new cell with the given character, colors, and flags.
	pub fn new(ch: impl Into<Grapheme>, fg: Color, bg: Color, flags: CellFlags) -> Self {
		Self { ch: ch.into(), fg, bg, flags }
	}

	/// Returns a reference to the cell's grapheme.
	pub fn ch(&self) -> &Grapheme { &self.ch }

	/// Returns the foreground color.
	pub fn fg(&self) -> Color { self.fg }

	/// Returns the background color.
	pub fn bg(&self) -> Color { self.bg }

	/// Returns the text attribute flags.
	pub fn flags(&self) -> CellFlags { self.flags }

	/// Returns `true` if the cell is visually empty (whitespace character with no background color).
	pub fn is_empty(&self) -> bool {
		self.ch.is_whitespace() && self.bg == Color::Reset
	}

	/// Set the background color (builder pattern).
	pub fn with_bg(mut self, bg: Color) -> Self {
		self.bg = bg;
		self
	}

	/// Set the foreground color (builder pattern).
	pub fn with_fg(mut self, fg: Color) -> Self {
		self.fg = fg;
		self
	}

	/// Set the text attribute flags (builder pattern).
	pub fn with_flags(mut self, flags: CellFlags) -> Self {
		self.flags = flags;
		self
	}

	/// Set the character (builder pattern).
	pub fn with_char(mut self, ch: char) -> Self {
		self.ch = ch.into();
		self
	}

	/// Set the background color in place.
	pub fn set_bg(&mut self, bg: Color) {
		self.bg = bg;
	}

	/// Set the foreground color in place.
	pub fn set_fg(&mut self, fg: Color) {
		self.fg = fg;
	}

	/// Set the text attribute flags in place.
	pub fn set_flags(&mut self, flags: CellFlags) {
		self.flags = flags;
	}

	/// Set the character in place.
	pub fn set_char(&mut self, ch: char) {
		self.ch = ch.into();
	}

	/// Append a codepoint to this cell's grapheme. Used for building up
	/// multi-codepoint sequences like ZWJ emoji.
	pub fn push_char(&mut self, ch: char) {
		self.ch.push_char(ch);
	}
}

impl Display for Cell {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		let mut styled = crossterm::style::style(&self.ch)
			.with(self.fg)
			.on(self.bg);

		if self.flags.contains(CellFlags::BOLD) {
			styled = styled.bold();
		}
		if self.flags.contains(CellFlags::ITALIC) {
			styled = styled.italic();
		}
		if self.flags.contains(CellFlags::UNDERLINE) {
			styled = styled.underlined();
		}
		if self.flags.contains(CellFlags::INVERSE) {
			styled = styled.reverse();
		}
		if self.flags.contains(CellFlags::HIDDEN) {
			styled = styled.hidden();
		}
		if self.flags.contains(CellFlags::STRIKETHROUGH) {
			styled = styled.crossed_out();
		}
		if self.flags.contains(CellFlags::DIM) {
			styled = styled.dim();
		}
		if self.flags.contains(CellFlags::BLINK) {
			styled = styled.slow_blink();
		}

		write!(f, "{styled}")
	}
}

impl From<char> for Cell {
	fn from(value: char) -> Self {
		Self::new(value, Color::Reset, Color::Reset, CellFlags::empty())
	}
}

//! # Cellophane
//!
//! A terminal animation framework for Rust. Implement one trait, get a complete
//! rendering pipeline with frame diffing, resize handling, and input forwarding.
//!
//! ## Core types
//!
//! - [`Animation`] - the trait you implement to define an animation
//! - [`Animator`] - drives your animation with a frame-rate-limited render loop
//! - [`Frame`] - a grid of styled [`Cell`]s representing one frame of output
//! - [`FrameBuilder`] - parses ANSI-escaped text into a `Frame`
//! - [`Cell`] - a single terminal cell with character, colors, and attributes
//! - [`Grapheme`] - a unicode grapheme cluster backed by `SmallVec<[char; 4]>`
//!
//! ## Quick start
//!
//! ```no_run
//! use std::time::Duration;
//! use cellophane::{Animation, Animator, Frame, Cell};
//! use crossterm::style::Color;
//!
//! struct Rainbow {
//!     tick: usize,
//!     rows: usize,
//!     cols: usize,
//! }
//!
//! impl Animation for Rainbow {
//!     fn init(&mut self, initial: Frame) {
//!         let (rows, cols) = initial.dims().unwrap_or((0, 0));
//!         self.rows = rows;
//!         self.cols = cols;
//!     }
//!
//!     fn update(&mut self, _dt: Duration) -> Frame {
//!         let mut frame = Frame::with_capacity(self.cols, self.rows);
//!         for row in 0..self.rows {
//!             for col in 0..self.cols {
//!                 let hue = ((col + row + self.tick) % 256) as u8;
//!                 if let Some(cell) = frame.get_cell_mut(row, col) {
//!                     *cell = Cell::default()
//!                         .with_bg(Color::Rgb { r: hue, g: 100, b: 255 - hue });
//!                 }
//!             }
//!         }
//!         self.tick += 1;
//!         frame
//!     }
//!
//!     fn is_done(&self) -> bool { false }
//!     fn resize(&mut self, w: usize, h: usize) {
//!         self.cols = w;
//!         self.rows = h;
//!     }
//! }
//!
//! fn main() -> std::io::Result<()> {
//!     let anim = Box::new(Rainbow { tick: 0, rows: 0, cols: 0 });
//!     let mut animator = Animator::enter_with(anim)?;
//!     loop {
//!         match animator.tick() {
//!             Ok(true) => continue,
//!             Ok(false) => break,
//!             Err(e) if e.kind() == std::io::ErrorKind::Interrupted => break,
//!             Err(e) => return Err(e),
//!         }
//!     }
//!     Ok(())
//! }
//! ```
//!
//! The `Animator` handles entering the alternate screen, enabling raw mode,
//! hiding the cursor, frame-rate limiting, frame diffing (only changed cells
//! are redrawn), terminal resize events, Ctrl+C handling, and terminal
//! restoration on drop.
//!
//! ## [ratatui](ratatui) Integration
//!
//! Enable with:
//! ```sh
//! cargo add cellophane --features=ratatui
//! ```
//!
//! This feature flag exposes the `AnimationWidget` struct, which allows `Frame` to be usable in the `ratatui` rendering pipeline.
//! `AnimationWidget` implements the `Widget` trait so it can be composed with other `ratatui` widgets as you would expect.
//! Here's an example using the `Block` widget:
//! ```rust
//! fn main() -> std::io::Result<()> {
//!   let mut anim = SomeAnimation::new();
//!   anim.init(anim.initial_frame());
//!
//!   ratatui::run(|terminal| {
//!     loop {
//!       terminal.draw(|f| {
//!         let chunks = Layout::default()
//!           .direction(Direction::Horizontal)
//!           .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
//!           .split(f.area());
//!
//!         let block = Block::default().title("Animation").borders(ratatui::widgets::Borders::ALL);
//!         let block_inner = block.inner(chunks[0]);
//!
//!         // resize the animation to match block_inner
//!         anim.resize(block_inner.width as usize, block_inner.height as usize);
//!         let anim_frame = anim.update(); // get the frame
//!
//!         // render the block widget, and the animation frame inside of it
//!         f.render_widget(block, chunks[0]);
//!         f.render_widget(AnimationWidget::new(&anim_frame), block_inner);
//!       })?;
//!
//!       if event::poll(Duration::from_millis(16))? {
//!         if event::read()?.is_key_press() {
//!           break Ok(());
//!         }
//!       }
//!     }
//!   })
//! }
//! ```

pub(crate) mod animator;
pub(crate) mod frame;

#[cfg(feature = "ratatui")]
pub(crate) mod ratatui_widget;

pub use animator::{Animation, Animator};
pub use crossterm;
pub use frame::{Cell, CellFlags, Frame, FrameBuilder, Grapheme, to_graphemes};
#[cfg(feature = "ratatui")]
pub use ratatui_widget::AnimationWidget;

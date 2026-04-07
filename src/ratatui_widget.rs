use ratatui::macros::ratatui_core;

use crate::CellFlags;

impl crate::Frame {
	/// Create a frame using a [Rect](ratatui::prelude::Rect) as a base.
	/// The created frame will have the same dimensions as the [Rect](ratatui::prelude::Rect).
  pub fn from_rect(rect: ratatui::prelude::Rect) -> Self {
    Self::with_capacity(rect.width as usize, rect.height as usize)
  }
}

/// A ratatui widget that renders an animation frame.
///
/// This allows you to use ratatui's widget rendering instead of the rendering pipeline provided by [Animator](crate::Animator)
/// This comes at the cost of having to manually manage the [Animation's](crate::Animation) update(), resize(), and on_event() methods.
/// The [Frame](crate::Frame) returned by the update() method is automatically mapped to the Rect/Buffer provided in ratatui::Widget::render().
///
/// This example shows how [Animations](crate::Animation) can be composed with other ratatui widgets:
/// ```
///let mut anim = SomeAnimation::new();
///anim.init(anim.initial_frame());
///
///ratatui::run(|terminal| {
///    let start = std::time::Instant::now();
///    loop {
///        terminal.draw(|f| {
///            let chunks = Layout::default()
///                .direction(Direction::Horizontal)
///                .constraints([Constraint::Percentage(50), Constraint::Percentage(50)])
///                .split(f.area());
///
///            let block = Block::default().title("Animation").borders(ratatui::widgets::Borders::ALL);
///            let block_inner = block.inner(chunks[0]);
///
///            // resize the animation to match block_inner
///            anim.resize(block_inner.width as usize, block_inner.height as usize);
///            let anim_frame = anim.update(); // get the frame
///
///            // render the block widget, and the animation frame inside of it
///            f.render_widget(block, chunks[0]);
///            f.render_widget(AnimationWidget::new(&anim_frame), block_inner);
///        })?;
///
///        if event::poll(Duration::from_millis(16))? {
///            if event::read()?.is_key_press() {
///                break Ok(());
///            }
///        }
///    }
///})
/// ```
pub struct AnimationWidget<'a> {
  frame: &'a crate::Frame,
}

impl<'a> AnimationWidget<'a> {
	/// Creates a new [AnimationWidget](AnimationWidget) that holds a reference to a [Frame](crate::Frame).
  pub fn new(frame: &'a crate::Frame) -> Self {
    Self { frame }
  }
}

fn crossterm_color_to_ratatui_color(color: crossterm::style::Color) -> ratatui::prelude::Color {
  match color {
    crossterm::style::Color::Reset => ratatui_core::style::Color::Reset,
    crossterm::style::Color::Black => ratatui_core::style::Color::Black,
    crossterm::style::Color::DarkGrey => ratatui_core::style::Color::DarkGray,
    crossterm::style::Color::Red => ratatui_core::style::Color::LightRed,
    crossterm::style::Color::DarkRed => ratatui_core::style::Color::Red,
    crossterm::style::Color::Green => ratatui_core::style::Color::LightGreen,
    crossterm::style::Color::DarkGreen => ratatui_core::style::Color::Green,
    crossterm::style::Color::Yellow => ratatui_core::style::Color::LightYellow,
    crossterm::style::Color::DarkYellow => ratatui_core::style::Color::Yellow,
    crossterm::style::Color::Blue => ratatui_core::style::Color::LightBlue,
    crossterm::style::Color::DarkBlue => ratatui_core::style::Color::Blue,
    crossterm::style::Color::Magenta => ratatui_core::style::Color::LightMagenta,
    crossterm::style::Color::DarkMagenta => ratatui_core::style::Color::Magenta,
    crossterm::style::Color::Cyan => ratatui_core::style::Color::LightCyan,
    crossterm::style::Color::DarkCyan => ratatui_core::style::Color::Cyan,
    crossterm::style::Color::White => ratatui_core::style::Color::White,
    crossterm::style::Color::Grey => ratatui_core::style::Color::Gray,
    crossterm::style::Color::Rgb { r, g, b } => ratatui_core::style::Color::Rgb(r, g, b),
    crossterm::style::Color::AnsiValue(n) => ratatui_core::style::Color::Indexed(n),
  }
}

impl From<CellFlags> for ratatui::style::Style {
  fn from(value: CellFlags) -> Self {
    let mut style = ratatui::style::Style::default();

    if value.contains(CellFlags::BOLD) {
      style = style.add_modifier(ratatui::style::Modifier::BOLD);
    }
    if value.contains(CellFlags::ITALIC) {
      style = style.add_modifier(ratatui::style::Modifier::ITALIC);
    }
    if value.contains(CellFlags::UNDERLINE) {
      style = style.add_modifier(ratatui::style::Modifier::UNDERLINED);
    }
    if value.contains(CellFlags::INVERSE) {
      style = style.add_modifier(ratatui::style::Modifier::REVERSED);
    }
    if value.contains(CellFlags::HIDDEN) {
      style = style.add_modifier(ratatui::style::Modifier::HIDDEN);
    }
    if value.contains(CellFlags::STRIKETHROUGH) {
      style = style.add_modifier(ratatui::style::Modifier::CROSSED_OUT);
    }
    if value.contains(CellFlags::DIM) {
      style = style.add_modifier(ratatui::style::Modifier::DIM);
    }
    if value.contains(CellFlags::BLINK) {
      style = style.add_modifier(ratatui::style::Modifier::SLOW_BLINK);
    }

    style
  }
}

impl From<crate::Cell> for ratatui::buffer::Cell {
  fn from(value: crate::Cell) -> Self {
    From::from(&value)
  }
}

impl From<&crate::Cell> for ratatui::buffer::Cell {
  fn from(value: &crate::Cell) -> Self {
    let mut rat_cell = Self::default();
    let symbol = value.ch().to_string();
    let fg = crossterm_color_to_ratatui_color(value.fg());
    let bg = crossterm_color_to_ratatui_color(value.bg());
    let flags = value.flags();

    rat_cell.set_symbol(&symbol);
    rat_cell.set_fg(fg);
    rat_cell.set_bg(bg);
    rat_cell.set_style(flags);

    rat_cell
  }
}

impl<'a> ratatui::widgets::Widget for AnimationWidget<'a> {
  fn render(self, area: ratatui::prelude::Rect, buf: &mut ratatui::prelude::Buffer)
  where
    Self: Sized,
  {
    let frame = self.frame;
    let height = frame.height().min(area.height as usize);
    let width = frame.width().min(area.width as usize);

    for row in 0..height {
      for col in 0..width {
        let Some(cell) = frame.get_cell(row, col) else {
          continue;
        };
        let cell: ratatui::buffer::Cell = cell.into();
        let x = area.x + col as u16;
        let y = area.y + row as u16;
        let Some(buf_cell) = buf.cell_mut((x, y)) else {
          continue;
        };
        *buf_cell = cell;
      }
    }
  }
}

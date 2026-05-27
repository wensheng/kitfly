use std::io::{self, Write};

use anyhow::Result;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event, execute,
    terminal::{
        Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
        enable_raw_mode, size, window_size,
    },
};

use crate::args::CellPixels;

pub const RESERVED_UI_ROWS: u16 = 1;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerminalLayout {
    pub cols: u16,
    pub rows: u16,
    pub display_width_px: u32,
    pub display_height_px: u32,
    pub cell_width_px: f32,
    pub cell_height_px: f32,
    pub canvas: TerminalMetrics,
    pub status_row: Option<u16>,
}

impl TerminalLayout {
    pub fn current(fallback: CellPixels, resolution_scale: f32) -> Self {
        let (cols, rows) = size().unwrap_or((80, 24));
        Self::from_cells(cols, rows, fallback, resolution_scale)
    }

    pub fn from_cells(cols: u16, rows: u16, fallback: CellPixels, resolution_scale: f32) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let window = window_size().ok();
        let display_width_px = window
            .as_ref()
            .filter(|window| window.width > 0 && window.height > 0)
            .map(|window| u32::from(window.width))
            .unwrap_or_else(|| u32::from(cols) * u32::from(fallback.width.max(1)));
        let display_height_px = window
            .as_ref()
            .filter(|window| window.width > 0 && window.height > 0)
            .map(|window| u32::from(window.height))
            .unwrap_or_else(|| u32::from(rows) * u32::from(fallback.height.max(1)));
        Self::from_display_dimensions(
            cols,
            rows,
            display_width_px,
            display_height_px,
            resolution_scale,
        )
    }

    pub fn from_display_dimensions(
        cols: u16,
        rows: u16,
        display_width_px: u32,
        display_height_px: u32,
        resolution_scale: f32,
    ) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let display_width_px = display_width_px.max(1);
        let display_height_px = display_height_px.max(1);
        let ui_rows = rows.saturating_sub(1).min(RESERVED_UI_ROWS);
        let canvas_rows = rows.saturating_sub(ui_rows).max(1);
        let canvas_display_height_px = ((u64::from(display_height_px) * u64::from(canvas_rows))
            / u64::from(rows))
        .max(1) as u32;
        let status_row = (ui_rows >= 1).then_some(canvas_rows);

        Self {
            cols,
            rows,
            display_width_px,
            display_height_px,
            cell_width_px: display_width_px as f32 / f32::from(cols),
            cell_height_px: display_height_px as f32 / f32::from(rows),
            canvas: TerminalMetrics::from_display_dimensions(
                cols,
                canvas_rows,
                display_width_px,
                canvas_display_height_px,
                resolution_scale,
            ),
            status_row,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerminalMetrics {
    pub cols: u16,
    pub rows: u16,
    pub display_width_px: u32,
    pub display_height_px: u32,
    pub width_px: u32,
    pub height_px: u32,
    pub cell_width_px: f32,
    pub cell_height_px: f32,
}

impl TerminalMetrics {
    #[cfg(test)]
    pub fn from_dimensions(cols: u16, rows: u16, width_px: u32, height_px: u32) -> Self {
        Self::from_display_dimensions(cols, rows, width_px, height_px, 1.0)
    }

    pub fn from_display_dimensions(
        cols: u16,
        rows: u16,
        display_width_px: u32,
        display_height_px: u32,
        resolution_scale: f32,
    ) -> Self {
        let cols = cols.max(1);
        let rows = rows.max(1);
        let display_width_px = display_width_px.max(1);
        let display_height_px = display_height_px.max(1);
        let resolution_scale = if resolution_scale.is_finite() {
            resolution_scale.clamp(0.1, 1.0)
        } else {
            0.5
        };
        let width_px = ((display_width_px as f32) * resolution_scale)
            .round()
            .max(1.0) as u32;
        let height_px = ((display_height_px as f32) * resolution_scale)
            .round()
            .max(1.0) as u32;
        Self {
            cols,
            rows,
            display_width_px,
            display_height_px,
            width_px,
            height_px,
            cell_width_px: width_px as f32 / f32::from(cols),
            cell_height_px: height_px as f32 / f32::from(rows),
        }
    }
}

pub struct TerminalSession;

impl TerminalSession {
    pub fn enter() -> Result<Self> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        if let Err(error) = execute!(
            stdout,
            EnterAlternateScreen,
            Hide,
            Clear(ClearType::All),
            MoveTo(0, 0)
        ) {
            let _ = disable_raw_mode();
            return Err(error.into());
        }
        stdout.flush()?;
        drain_pending_events()?;
        Ok(Self)
    }
}

fn drain_pending_events() -> Result<()> {
    while event::poll(std::time::Duration::ZERO)? {
        let _ = event::read()?;
    }
    Ok(())
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let mut stdout = io::stdout();
        let _ = execute!(stdout, Show, LeaveAlternateScreen, MoveTo(0, 0));
        let _ = stdout.flush();
        let _ = disable_raw_mode();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_scale_from_dimensions() {
        let metrics = TerminalMetrics::from_dimensions(80, 24, 800, 480);
        assert_eq!(metrics.cols, 80);
        assert_eq!(metrics.rows, 24);
        assert_eq!(metrics.display_width_px, 800);
        assert_eq!(metrics.display_height_px, 480);
        assert_eq!(metrics.width_px, 800);
        assert_eq!(metrics.height_px, 480);
        assert_eq!(metrics.cell_width_px, 10.0);
        assert_eq!(metrics.cell_height_px, 20.0);
    }

    #[test]
    fn resolution_scale_reduces_canvas_size() {
        let metrics = TerminalMetrics::from_display_dimensions(80, 24, 800, 480, 0.5);
        assert_eq!(metrics.width_px, 400);
        assert_eq!(metrics.height_px, 240);
        assert_eq!(metrics.cell_width_px, 5.0);
        assert_eq!(metrics.cell_height_px, 10.0);
    }

    #[test]
    fn layout_reserves_one_status_row() {
        let layout = TerminalLayout::from_display_dimensions(80, 24, 800, 480, 1.0);
        assert_eq!(layout.canvas.cols, 80);
        assert_eq!(layout.canvas.rows, 23);
        assert_eq!(layout.status_row, Some(23));
        assert_eq!(layout.canvas.display_height_px, 460);
    }
}

use clap::Parser;

#[derive(Debug, Clone, Parser)]
#[command(version, about = "A Kitty graphics terminal flight demo")]
pub struct Args {
    /// Target frames per second for the terminal presentation loop.
    #[arg(long, default_value_t = 30, value_parser = clap::value_parser!(u32).range(1..=120))]
    pub fps: u32,

    /// Scale terminal pixel dimensions before rendering.
    #[arg(long, default_value_t = 0.5)]
    pub resolution_scale: f32,

    /// Fallback terminal cell width in pixels when the terminal does not report pixel size.
    #[arg(long, default_value_t = 10)]
    pub fallback_cell_width: u16,

    /// Fallback terminal cell height in pixels when the terminal does not report pixel size.
    #[arg(long, default_value_t = 20)]
    pub fallback_cell_height: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellPixels {
    pub width: u16,
    pub height: u16,
}

impl Args {
    pub fn fallback_cell_px(&self) -> CellPixels {
        CellPixels {
            width: self.fallback_cell_width.max(1),
            height: self.fallback_cell_height.max(1),
        }
    }
}

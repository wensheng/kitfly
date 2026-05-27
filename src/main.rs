mod args;
mod capture;
mod controls;
mod kitty;
mod plane_config;
mod scene;
mod terminal;

use std::{
    io::{self, Write},
    time::{Duration, Instant},
};

use anyhow::Result;
use bevy::{app::SubApps, prelude::*};
use clap::Parser;
use crossterm::{
    cursor::MoveTo,
    event::{self, Event, KeyEventKind},
    queue,
    style::Print,
    terminal::{Clear, ClearType},
};

use crate::{
    args::Args,
    capture::CapturedFrame,
    controls::{FlightCommand, FlightState, command_for_key},
    terminal::{TerminalLayout, TerminalSession},
};

fn main() -> Result<()> {
    let args = Args::parse();
    let frame_interval = Duration::from_secs_f64(1.0 / f64::from(args.fps));
    let fallback_cell_px = args.fallback_cell_px();
    let mut layout = TerminalLayout::current(fallback_cell_px, args.resolution_scale);

    let mut app = App::new();
    scene::configure_app(&mut app);
    app.add_plugins(capture::FrameCapturePlugin);
    let mut sub_apps = capture::finish_for_external_loop(&mut app);
    let (render_target, _target_handle) = capture::create_render_target(
        sub_apps.main.world_mut(),
        layout.canvas.width_px,
        layout.canvas.height_px,
    );
    scene::spawn_camera(sub_apps.main.world_mut(), render_target);

    let _terminal = TerminalSession::enter()?;
    run_loop(
        &mut sub_apps,
        &mut layout,
        fallback_cell_px,
        args.resolution_scale,
        frame_interval,
    )
}

fn run_loop(
    sub_apps: &mut SubApps,
    layout: &mut TerminalLayout,
    fallback_cell_px: args::CellPixels,
    resolution_scale: f32,
    frame_interval: Duration,
) -> Result<()> {
    let mut stdout = io::stdout().lock();
    let mut last_frame = Instant::now();
    let mut latest_frame: Option<CapturedFrame> = None;
    let mut quit = false;

    while !quit {
        drain_events(
            sub_apps.main.world_mut(),
            layout,
            fallback_cell_px,
            resolution_scale,
            &mut quit,
        )?;

        sub_apps.update();
        capture::wait_for_render_device(sub_apps.main.world());
        if let Some(frame) = capture::latest_frame(sub_apps.main.world()) {
            latest_frame = Some(frame);
        }

        if last_frame.elapsed() >= frame_interval {
            render_terminal(
                &mut stdout,
                latest_frame.as_ref(),
                *layout,
                sub_apps.main.world(),
            )?;
            last_frame = Instant::now();
        }

        let sleep_for = frame_interval
            .checked_sub(last_frame.elapsed())
            .unwrap_or(Duration::from_millis(1))
            .min(Duration::from_millis(4));
        std::thread::sleep(sleep_for);
    }

    Ok(())
}

fn drain_events(
    world: &mut World,
    layout: &mut TerminalLayout,
    fallback_cell_px: args::CellPixels,
    resolution_scale: f32,
    quit: &mut bool,
) -> Result<()> {
    while event::poll(Duration::ZERO)? {
        match event::read()? {
            Event::Key(key) if matches!(key.kind, KeyEventKind::Press | KeyEventKind::Repeat) => {
                if let Some(command) = command_for_key(key) {
                    match command {
                        FlightCommand::Quit => *quit = true,
                        FlightCommand::NextPlane => {
                            world.resource_mut::<scene::PlaneSelection>().next();
                        }
                        command => world.resource_mut::<FlightState>().apply_command(command),
                    }
                }
            }
            Event::Resize(cols, rows) => {
                *layout =
                    TerminalLayout::from_cells(cols, rows, fallback_cell_px, resolution_scale);
            }
            _ => {}
        }
    }
    Ok(())
}

fn render_terminal<W: Write>(
    writer: &mut W,
    frame: Option<&CapturedFrame>,
    layout: TerminalLayout,
    world: &World,
) -> Result<()> {
    queue!(writer, MoveTo(0, 0))?;
    if let Some(frame) = frame {
        kitty::write_rgba_frame(
            writer,
            &frame.pixels,
            frame.width,
            frame.height,
            u32::from(layout.canvas.cols),
            u32::from(layout.canvas.rows),
            true,
        )?;
    }
    render_status(writer, layout, world)?;
    Ok(())
}

fn render_status<W: Write>(writer: &mut W, layout: TerminalLayout, world: &World) -> Result<()> {
    let Some(row) = layout.status_row else {
        return Ok(());
    };
    let state = world.resource::<FlightState>();
    let plane = world.resource::<scene::PlaneSelection>();
    let text = format!(
        "kitfly | follow cam | {} | speed {:>4.1} | pitch {:>5.1} heading {:>5.1} | visual yaw {:>5.1} roll {:>5.1} | arrows fly  w/x speed  s plane  space reset  q quit",
        plane.current_name(),
        state.speed,
        state.pitch.to_degrees(),
        state.heading.to_degrees(),
        state.visual_yaw.to_degrees(),
        state.visual_roll.to_degrees(),
    );
    queue!(
        writer,
        MoveTo(0, row),
        Clear(ClearType::CurrentLine),
        Print(truncate_to_cols(&text, layout.cols))
    )?;
    writer.flush()?;
    Ok(())
}

fn truncate_to_cols(text: &str, cols: u16) -> String {
    text.chars().take(cols as usize).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capture::MainWorldReceiver;

    #[test]
    fn truncates_status_to_terminal_width() {
        assert_eq!(truncate_to_cols("abcdef", 3), "abc");
    }

    #[test]
    fn main_world_receiver_is_sendable_resource_shape() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<MainWorldReceiver>();
    }
}

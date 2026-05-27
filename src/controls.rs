use std::f32::consts::{FRAC_PI_2, PI};

use bevy::prelude::*;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub const DEFAULT_SPEED: f32 = 7.0;
const MIN_SPEED: f32 = 0.0;
const MAX_SPEED: f32 = 40.0;
const SPEED_STEP: f32 = 1.0;
const PITCH_STEP: f32 = 0.06;
const TURN_STEP: f32 = 0.1;
const PITCH_IMPULSE: f32 = 0.24;
const ROLL_IMPULSE: f32 = 0.32;
const YAW_IMPULSE: f32 = 0.08;
const RETURN_RATE: f32 = 4.8;
const CAMERA_DISTANCE: f32 = 10.0;
const CAMERA_HEIGHT: f32 = 4.5;
const MAX_PITCH: f32 = FRAC_PI_2 * 0.55;
const MAX_VISUAL_PITCH: f32 = FRAC_PI_2 * 0.42;
const MAX_VISUAL_YAW: f32 = PI * 0.18;
const MAX_VISUAL_ROLL: f32 = PI * 0.32;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FlightCommand {
    PitchUp,
    PitchDown,
    YawLeft,
    YawRight,
    RollLeft,
    RollRight,
    SpeedUp,
    SpeedDown,
    NextPlane,
    Reset,
    Quit,
}

#[derive(Debug, Clone, Resource)]
pub struct FlightState {
    pub position: Vec3,
    pub heading: f32,
    pub pitch: f32,
    pub visual_pitch: f32,
    pub visual_yaw: f32,
    pub visual_roll: f32,
    pub speed: f32,
}

impl Default for FlightState {
    fn default() -> Self {
        Self {
            position: Vec3::new(0.0, 4.0, 8.0),
            heading: 0.0,
            pitch: 0.0,
            visual_pitch: 0.0,
            visual_yaw: 0.0,
            visual_roll: 0.0,
            speed: DEFAULT_SPEED,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FlightPose {
    pub airplane: Transform,
    pub camera: Transform,
}

impl FlightState {
    pub fn apply_command(&mut self, command: FlightCommand) {
        match command {
            FlightCommand::PitchUp => {
                self.pitch += PITCH_STEP;
                self.visual_pitch += PITCH_IMPULSE;
            }
            FlightCommand::PitchDown => {
                self.pitch -= PITCH_STEP;
                self.visual_pitch -= PITCH_IMPULSE;
            }
            FlightCommand::YawLeft => {
                self.heading += TURN_STEP;
                self.visual_yaw += YAW_IMPULSE;
                self.visual_roll += ROLL_IMPULSE;
            }
            FlightCommand::YawRight => {
                self.heading -= TURN_STEP;
                self.visual_yaw -= YAW_IMPULSE;
                self.visual_roll -= ROLL_IMPULSE;
            }
            FlightCommand::RollLeft => self.visual_roll += ROLL_IMPULSE,
            FlightCommand::RollRight => self.visual_roll -= ROLL_IMPULSE,
            FlightCommand::SpeedUp => self.speed += SPEED_STEP,
            FlightCommand::SpeedDown => self.speed -= SPEED_STEP,
            FlightCommand::NextPlane => {}
            FlightCommand::Reset => *self = Self::default(),
            FlightCommand::Quit => {}
        }
        self.clamp();
    }

    pub fn advance(&mut self, dt: f32) -> FlightPose {
        self.clamp();
        let dt = dt.max(0.0);
        let flight_rotation = self.flight_rotation();
        self.position += flight_rotation * -Vec3::Z * self.speed * dt;
        self.position.y = self.position.y.clamp(1.4, 32.0);
        self.return_to_neutral(dt);
        self.pose()
    }

    pub fn pose(&self) -> FlightPose {
        let heading_rotation = self.heading_rotation();
        let flight_rotation = self.flight_rotation();
        let visual_rotation = self.visual_rotation();
        let airplane_rotation = flight_rotation * visual_rotation;
        let airplane = Transform::from_translation(self.position).with_rotation(airplane_rotation);

        let forward = heading_rotation * -Vec3::Z;
        let camera_position = self.position - forward * CAMERA_DISTANCE + Vec3::Y * CAMERA_HEIGHT;
        let camera =
            Transform::from_translation(camera_position).looking_at(self.position, Vec3::Y);

        FlightPose { airplane, camera }
    }

    pub fn heading_rotation(&self) -> Quat {
        Quat::from_rotation_y(self.heading)
    }

    pub fn flight_rotation(&self) -> Quat {
        self.heading_rotation() * Quat::from_rotation_x(self.pitch)
    }

    pub fn visual_rotation(&self) -> Quat {
        Quat::from_euler(
            EulerRot::YXZ,
            self.visual_yaw,
            self.visual_pitch,
            self.visual_roll,
        )
    }

    fn clamp(&mut self) {
        self.pitch = self.pitch.clamp(-MAX_PITCH, MAX_PITCH);
        self.visual_pitch = self.visual_pitch.clamp(-MAX_VISUAL_PITCH, MAX_VISUAL_PITCH);
        self.visual_yaw = self.visual_yaw.clamp(-MAX_VISUAL_YAW, MAX_VISUAL_YAW);
        self.visual_roll = self.visual_roll.clamp(-MAX_VISUAL_ROLL, MAX_VISUAL_ROLL);
        self.speed = self.speed.clamp(MIN_SPEED, MAX_SPEED);
    }

    fn return_to_neutral(&mut self, dt: f32) {
        let retention = (-RETURN_RATE * dt).exp();
        self.visual_pitch = decay_to_zero(self.visual_pitch, retention);
        self.visual_yaw = decay_to_zero(self.visual_yaw, retention);
        self.visual_roll = decay_to_zero(self.visual_roll, retention);
    }
}

fn decay_to_zero(value: f32, retention: f32) -> f32 {
    let decayed = value * retention;
    if decayed.abs() < 0.002 { 0.0 } else { decayed }
}

pub fn command_for_key(key: KeyEvent) -> Option<FlightCommand> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
        return Some(FlightCommand::Quit);
    }

    match key.code {
        KeyCode::Esc | KeyCode::Char('q') => Some(FlightCommand::Quit),
        KeyCode::Up => Some(FlightCommand::PitchUp),
        KeyCode::Down => Some(FlightCommand::PitchDown),
        KeyCode::Left => Some(FlightCommand::YawLeft),
        KeyCode::Right => Some(FlightCommand::YawRight),
        KeyCode::Char('a') | KeyCode::Char('A') => Some(FlightCommand::RollLeft),
        KeyCode::Char('d') | KeyCode::Char('D') => Some(FlightCommand::RollRight),
        KeyCode::Char('w') | KeyCode::Char('W') => Some(FlightCommand::SpeedUp),
        KeyCode::Char('x') | KeyCode::Char('X') => Some(FlightCommand::SpeedDown),
        KeyCode::Char('s') | KeyCode::Char('S') => Some(FlightCommand::NextPlane),
        KeyCode::Char(' ') => Some(FlightCommand::Reset),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    #[test]
    fn maps_keyboard_controls() {
        assert_eq!(
            command_for_key(key(KeyCode::Up)),
            Some(FlightCommand::PitchUp)
        );
        assert_eq!(
            command_for_key(key(KeyCode::Char('w'))),
            Some(FlightCommand::SpeedUp)
        );
        assert_eq!(
            command_for_key(key(KeyCode::Char('x'))),
            Some(FlightCommand::SpeedDown)
        );
        assert_eq!(
            command_for_key(key(KeyCode::Char('s'))),
            Some(FlightCommand::NextPlane)
        );
        assert_eq!(
            command_for_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL)),
            Some(FlightCommand::Quit)
        );
        assert_eq!(command_for_key(key(KeyCode::Char('v'))), None);
    }

    #[test]
    fn speed_and_visual_attitude_are_clamped() {
        let mut state = FlightState::default();
        for _ in 0..100 {
            state.apply_command(FlightCommand::SpeedUp);
            state.apply_command(FlightCommand::PitchUp);
            state.apply_command(FlightCommand::RollLeft);
        }
        assert_eq!(state.speed, MAX_SPEED);
        assert_eq!(state.pitch, MAX_PITCH);
        assert_eq!(state.visual_pitch, MAX_VISUAL_PITCH);
        assert_eq!(state.visual_roll, MAX_VISUAL_ROLL);

        for _ in 0..100 {
            state.apply_command(FlightCommand::SpeedDown);
            state.apply_command(FlightCommand::PitchDown);
            state.apply_command(FlightCommand::RollRight);
        }
        assert_eq!(state.speed, MIN_SPEED);
        assert_eq!(state.pitch, -MAX_PITCH);
        assert_eq!(state.visual_pitch, -MAX_VISUAL_PITCH);
        assert_eq!(state.visual_roll, -MAX_VISUAL_ROLL);
    }

    #[test]
    fn visual_attitude_returns_to_neutral() {
        let mut state = FlightState::default();
        state.apply_command(FlightCommand::PitchUp);
        state.apply_command(FlightCommand::YawLeft);
        assert!(state.visual_pitch > 0.0);
        assert!(state.visual_roll > 0.0);

        for _ in 0..90 {
            state.advance(1.0 / 60.0);
        }

        assert!(state.visual_pitch.abs() < 0.01);
        assert!(state.visual_yaw.abs() < 0.01);
        assert!(state.visual_roll.abs() < 0.01);
    }

    #[test]
    fn camera_follows_above_and_behind_airplane() {
        let state = FlightState::default();
        let pose = state.pose();
        assert!(pose.camera.translation.y > pose.airplane.translation.y);
        assert!(pose.camera.translation.z > pose.airplane.translation.z);
        assert!(
            (pose.camera.translation.y - pose.airplane.translation.y - CAMERA_HEIGHT).abs() < 0.001
        );
    }

    #[test]
    fn yaw_changes_heading_and_movement_direction() {
        let mut state = FlightState::default();
        let start = state.position;
        state.apply_command(FlightCommand::YawLeft);
        assert!(state.heading > 0.0);

        state.advance(1.0);

        assert!(state.position.x < start.x);
        assert!(state.position.z < start.z);
    }

    #[test]
    fn pitch_changes_attitude_and_vertical_movement() {
        let mut state = FlightState::default();
        let start = state.position;
        state.apply_command(FlightCommand::PitchUp);
        assert!(state.pitch > 0.0);

        state.advance(1.0);

        assert!(state.position.y > start.y);
    }
}

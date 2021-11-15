//! User input event types.

/// Describes, which buttons are pressed down during a mouse drag.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct MouseDragSettings {
    pub button: MouseButton,
    pub shift_pressed: bool,
    pub ctrl_pressed: bool,
    pub alt_pressed: bool,
}

/// Mouse buttons
#[derive(Copy, Clone, Eq, PartialEq, Debug)]
pub enum MouseButton {
    Left,
    Middle,
    Right,
    Other,
}

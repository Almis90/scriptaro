use core_graphics::event::{CGEventFlags, KeyCode};
use scriptaro_core::{Key, Modifier};

pub fn flags(modifiers: &[Modifier]) -> CGEventFlags {
    modifiers
        .iter()
        .fold(CGEventFlags::empty(), |flags, modifier| {
            flags
                | match modifier {
                    Modifier::Primary | Modifier::Super => CGEventFlags::CGEventFlagCommand,
                    Modifier::Control => CGEventFlags::CGEventFlagControl,
                    Modifier::Alt => CGEventFlags::CGEventFlagAlternate,
                    Modifier::Shift => CGEventFlags::CGEventFlagShift,
                }
        })
}

pub fn keycode(key: Key) -> u16 {
    match key {
        Key::A => KeyCode::ANSI_A,
        Key::B => KeyCode::ANSI_B,
        Key::C => KeyCode::ANSI_C,
        Key::D => KeyCode::ANSI_D,
        Key::E => KeyCode::ANSI_E,
        Key::F => KeyCode::ANSI_F,
        Key::G => KeyCode::ANSI_G,
        Key::H => KeyCode::ANSI_H,
        Key::I => KeyCode::ANSI_I,
        Key::J => KeyCode::ANSI_J,
        Key::K => KeyCode::ANSI_K,
        Key::L => KeyCode::ANSI_L,
        Key::M => KeyCode::ANSI_M,
        Key::N => KeyCode::ANSI_N,
        Key::O => KeyCode::ANSI_O,
        Key::P => KeyCode::ANSI_P,
        Key::Q => KeyCode::ANSI_Q,
        Key::R => KeyCode::ANSI_R,
        Key::S => KeyCode::ANSI_S,
        Key::T => KeyCode::ANSI_T,
        Key::U => KeyCode::ANSI_U,
        Key::V => KeyCode::ANSI_V,
        Key::W => KeyCode::ANSI_W,
        Key::X => KeyCode::ANSI_X,
        Key::Y => KeyCode::ANSI_Y,
        Key::Z => KeyCode::ANSI_Z,
        Key::Digit0 => KeyCode::ANSI_0,
        Key::Digit1 => KeyCode::ANSI_1,
        Key::Digit2 => KeyCode::ANSI_2,
        Key::Digit3 => KeyCode::ANSI_3,
        Key::Digit4 => KeyCode::ANSI_4,
        Key::Digit5 => KeyCode::ANSI_5,
        Key::Digit6 => KeyCode::ANSI_6,
        Key::Digit7 => KeyCode::ANSI_7,
        Key::Digit8 => KeyCode::ANSI_8,
        Key::Digit9 => KeyCode::ANSI_9,
        Key::Enter => KeyCode::RETURN,
        Key::Tab => KeyCode::TAB,
        Key::Space => KeyCode::SPACE,
        Key::Backspace => KeyCode::DELETE,
        Key::Delete => KeyCode::FORWARD_DELETE,
        Key::Escape => KeyCode::ESCAPE,
        Key::Left => KeyCode::LEFT_ARROW,
        Key::Right => KeyCode::RIGHT_ARROW,
        Key::Up => KeyCode::UP_ARROW,
        Key::Down => KeyCode::DOWN_ARROW,
        Key::Home => KeyCode::HOME,
        Key::End => KeyCode::END,
        Key::PageUp => KeyCode::PAGE_UP,
        Key::PageDown => KeyCode::PAGE_DOWN,
        Key::Minus => KeyCode::ANSI_MINUS,
        Key::Equal => KeyCode::ANSI_EQUAL,
        Key::LeftBracket => KeyCode::ANSI_LEFT_BRACKET,
        Key::RightBracket => KeyCode::ANSI_RIGHT_BRACKET,
        Key::Backslash => KeyCode::ANSI_BACKSLASH,
        Key::Semicolon => KeyCode::ANSI_SEMICOLON,
        Key::Quote => KeyCode::ANSI_QUOTE,
        Key::Comma => KeyCode::ANSI_COMMA,
        Key::Period => KeyCode::ANSI_PERIOD,
        Key::Slash => KeyCode::ANSI_SLASH,
        Key::Backtick => KeyCode::ANSI_GRAVE,
        Key::F1 => KeyCode::F1,
        Key::F2 => KeyCode::F2,
        Key::F3 => KeyCode::F3,
        Key::F4 => KeyCode::F4,
        Key::F5 => KeyCode::F5,
        Key::F6 => KeyCode::F6,
        Key::F7 => KeyCode::F7,
        Key::F8 => KeyCode::F8,
        Key::F9 => KeyCode::F9,
        Key::F10 => KeyCode::F10,
        Key::F11 => KeyCode::F11,
        Key::F12 => KeyCode::F12,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn primary_is_command_and_aliases_do_not_duplicate_flags() {
        assert_eq!(flags(&[Modifier::Primary]), flags(&[Modifier::Super]));
        assert_eq!(
            flags(&[Modifier::Primary, Modifier::Super]),
            flags(&[Modifier::Primary])
        );
        assert_ne!(flags(&[Modifier::Primary]), flags(&[Modifier::Control]));
    }
    #[test]
    fn backspace_and_forward_delete_are_distinct() {
        assert_ne!(keycode(Key::Backspace), keycode(Key::Delete));
    }
}

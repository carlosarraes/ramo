use std::io::{Cursor, Read};

use crate::core::input::PatchSource;

use super::{LoadError, LoadOutcome, patch};

pub(super) fn load(stdin: &mut dyn Read) -> Result<LoadOutcome, LoadError> {
    let mut raw = String::new();
    stdin.read_to_string(&mut raw).map_err(LoadError::Stdin)?;
    if !looks_like_patch(&raw) {
        return Ok(LoadOutcome::PlainText(raw));
    }

    let mut input = Cursor::new(raw);
    patch::load(&PatchSource::Stdin, &mut input)
        .map(Box::new)
        .map(LoadOutcome::Review)
}

pub fn looks_like_patch(input: &str) -> bool {
    let normalized = sanitize_terminal_text(input, false);
    let mut old_header = false;
    let mut new_header = false;

    for line in normalized.lines() {
        if line.starts_with("diff --git ") || line.starts_with("@@ ") {
            return true;
        }
        old_header |= line.starts_with("--- ");
        new_header |= line.starts_with("+++ ");
    }

    old_header && new_header
}

pub fn sanitize_terminal_text(input: &str, preserve_sgr: bool) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Ground,
        Escape,
        Csi,
        Osc,
        OscEscape,
    }

    let normalized = input.replace("\r\n", "\n");
    let mut output = String::with_capacity(normalized.len());
    let mut state = State::Ground;
    let mut csi = String::new();

    for character in normalized.chars() {
        state = match state {
            State::Ground if character == '\u{1b}' => State::Escape,
            State::Ground => {
                if character == '\n' || character == '\t' || !character.is_control() {
                    output.push(character);
                }
                State::Ground
            }
            State::Escape if character == '[' => {
                csi.clear();
                State::Csi
            }
            State::Escape if character == ']' => State::Osc,
            State::Escape => State::Ground,
            State::Csi if ('@'..='~').contains(&character) => {
                if preserve_sgr
                    && character == 'm'
                    && csi
                        .chars()
                        .all(|value| value.is_ascii_digit() || value == ';')
                {
                    output.push_str("\u{1b}[");
                    output.push_str(&csi);
                    output.push('m');
                }
                State::Ground
            }
            State::Csi => {
                csi.push(character);
                State::Csi
            }
            State::Osc if character == '\u{7}' => State::Ground,
            State::Osc if character == '\u{1b}' => State::OscEscape,
            State::Osc => State::Osc,
            State::OscEscape if character == '\\' => State::Ground,
            State::OscEscape if character == '\u{1b}' => State::OscEscape,
            State::OscEscape => State::Osc,
        };
    }

    output
}

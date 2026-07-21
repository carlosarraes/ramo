use std::fs;
use std::io::Read;

use crate::core::changeset::Changeset;
use crate::core::input::PatchSource;
use crate::diff::parser::parse_unified_diff;

use super::{LoadError, LoadedReview, ReloadPlan};

pub(super) fn load(source: &PatchSource, stdin: &mut dyn Read) -> Result<LoadedReview, LoadError> {
    let (source_label, title, raw) = match source {
        PatchSource::Stdin => {
            let mut raw = String::new();
            stdin.read_to_string(&mut raw).map_err(LoadError::Stdin)?;
            (
                "patch stdin".to_string(),
                "Patch from stdin".to_string(),
                raw,
            )
        }
        PatchSource::File(path) => {
            let raw = fs::read_to_string(path).map_err(|source| LoadError::Io {
                path: path.clone(),
                source,
            })?;
            (
                format!("patch {}", path.display()),
                path.file_name().map_or_else(
                    || path.display().to_string(),
                    |name| name.to_string_lossy().into(),
                ),
                raw,
            )
        }
    };

    let normalized = normalize_patch_text(&raw);
    if normalized.trim().is_empty() {
        return Err(LoadError::EmptyInput);
    }
    let files = parse_unified_diff(&normalized);
    if files.is_empty() {
        return Err(LoadError::InvalidPatch {
            source_label: source_label.clone(),
        });
    }
    Ok(LoadedReview {
        changeset: Changeset::new(source_label, title, files),
        reload_plan: ReloadPlan::None,
    })
}

pub fn normalize_patch_text(input: &str) -> String {
    #[derive(Clone, Copy)]
    enum State {
        Ground,
        Escape,
        Csi,
        Osc,
        OscEscape,
    }

    let mut state = State::Ground;
    let mut output = String::with_capacity(input.len());
    for character in input.replace("\r\n", "\n").chars() {
        state = match state {
            State::Ground if character == '\u{1b}' => State::Escape,
            State::Ground => {
                output.push(character);
                State::Ground
            }
            State::Escape if character == '[' => State::Csi,
            State::Escape if character == ']' => State::Osc,
            State::Escape => State::Ground,
            State::Csi if ('@'..='~').contains(&character) => State::Ground,
            State::Csi => State::Csi,
            State::Osc if character == '\u{7}' => State::Ground,
            State::Osc if character == '\u{1b}' => State::OscEscape,
            State::Osc => State::Osc,
            State::OscEscape if character == '\\' => State::Ground,
            State::OscEscape => State::Osc,
        };
    }
    output
}

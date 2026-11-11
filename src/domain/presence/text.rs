//! Texts for the presence: file labels, agent lines and Discord-sized cuts.

use crate::domain::agents::AgentSummary;
use crate::domain::event::CursorPosition;
use std::ffi::OsStr;
use std::path::Path;

/// Discord cuts details, state and the two asset texts at 128 characters.
pub(crate) const MAX_TEXT_CHARS: usize = 128;

/// Discord rejects texts over 128 characters; cut on char boundaries and mark
/// the cut so a multi-byte character is never split.
pub(crate) fn truncate(mut text: String) -> String {
    if text.chars().count() > MAX_TEXT_CHARS {
        text = text.chars().take(MAX_TEXT_CHARS - 1).collect();
        text.push('…');
    }
    text
}

pub(crate) fn file_label(path: &Path, cursor: Option<CursorPosition>, show_line: bool) -> String {
    let name = path
        .file_name()
        .and_then(OsStr::to_str)
        .map_or_else(|| path.display().to_string(), str::to_owned);
    match (show_line, cursor) {
        (true, Some(position)) if position.line > 0 => format!("{name}:{}", position.line),
        _ => name,
    }
}

/// Waiting wins over working: an agent blocked on input is the most useful
/// thing to report. `None` while nothing is active.
pub(crate) fn agent_line(summary: &AgentSummary) -> Option<String> {
    if summary.waiting > 0 {
        Some(match summary.waiting {
            1 => "1 agent waiting for input".to_owned(),
            waiting => format!("{waiting} agents waiting for input"),
        })
    } else if summary.working > 0 {
        Some(match summary.working {
            1 => "1 agent working".to_owned(),
            working => format!("{working} agents working"),
        })
    } else {
        None
    }
}

/// The harness name is private enough to appear only for a lone agent.
pub(crate) fn single_harness_text(summary: &AgentSummary) -> Option<String> {
    if summary.count() != 1 {
        return None;
    }
    let harness = harness_label(summary.only_harness.as_deref()?);
    let doing = if summary.waiting == 1 {
        "waiting for input"
    } else {
        "working"
    };
    Some(format!("{harness} {doing}"))
}

/// The app serialises the harness as `ClaudeCode`, `Codex`, `Opencode`,
/// `Shell`, `Gateway` or `Acp:<id>`.
fn harness_label(harness: &str) -> &str {
    match harness {
        "ClaudeCode" => "Claude Code",
        "Opencode" => "opencode",
        other => other.strip_prefix("Acp:").map_or(other, |_| "ACP agent"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texts_are_cut_on_boundaries_and_file_labels_stay_short() {
        assert_eq!(truncate("main.rs".to_owned()), "main.rs");

        let cut = truncate("é".repeat(MAX_TEXT_CHARS + 20));
        assert_eq!(cut.chars().count(), MAX_TEXT_CHARS);
        assert!(cut.ends_with('…'));

        let path = Path::new("/code/api/src/main.rs");
        let cursor = Some(CursorPosition {
            line: 42,
            column: 7,
        });
        assert_eq!(file_label(path, cursor, false), "main.rs");
        assert_eq!(file_label(path, cursor, true), "main.rs:42");
        assert_eq!(file_label(path, None, true), "main.rs");
    }
}

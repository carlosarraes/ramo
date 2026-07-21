use crate::core::changeset::stable_file_id;

use super::model::{DiffFile, DiffLine, FileChangeKind, FileStats, Hunk, LineType, SourceSpec};

fn strip_ansi(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' && chars.peek() == Some(&'[') {
            chars.next();
            for c in chars.by_ref() {
                if matches!(c, '\x40'..='\x7e') {
                    break;
                }
            }
        } else {
            out.push(c);
        }
    }
    out
}

pub fn parse_unified_diff(input: &str) -> Vec<DiffFile> {
    let stripped = strip_ansi(input);
    let mut files = Vec::new();
    let lines: Vec<&str> = stripped.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].starts_with("diff --git ") {
            let (file, consumed) = parse_file(&lines[i..]);
            if let Some(f) = file {
                files.push(f);
            }
            i += consumed;
        } else {
            i += 1;
        }
    }

    files
}

fn parse_file(lines: &[&str]) -> (Option<DiffFile>, usize) {
    let file_len = lines
        .iter()
        .skip(1)
        .position(|line| line.starts_with("diff --git "))
        .map_or(lines.len(), |index| index + 1);
    let lines = &lines[..file_len];
    let mut i = 0;
    let diff_line = lines[i];
    i += 1;

    // Extract paths from "diff --git a/path b/path"
    let (old_path, new_path) = parse_diff_git_line(diff_line);

    let mut is_new = false;
    let mut is_deleted = false;
    let mut is_renamed = false;
    let mut is_copied = false;
    let mut is_binary = false;
    let mut actual_old_path = old_path.clone();
    let mut actual_new_path = new_path.clone();

    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("---") || line.starts_with("@@") || line.starts_with("diff --git ") {
            break;
        }
        if line.starts_with("new file") {
            is_new = true;
        } else if line.starts_with("deleted file") {
            is_deleted = true;
        } else if line.starts_with("Binary files") || line.starts_with("GIT binary patch") {
            is_binary = true;
        } else if line.starts_with("rename from ") {
            is_renamed = true;
            actual_old_path = unquote_git_path(line.strip_prefix("rename from ").unwrap());
        } else if line.starts_with("rename to ") {
            is_renamed = true;
            actual_new_path = unquote_git_path(line.strip_prefix("rename to ").unwrap());
        } else if line.starts_with("copy from ") {
            is_copied = true;
            actual_old_path = unquote_git_path(line.strip_prefix("copy from ").unwrap());
        } else if line.starts_with("copy to ") {
            is_copied = true;
            actual_new_path = unquote_git_path(line.strip_prefix("copy to ").unwrap());
        }
        i += 1;
    }

    if is_binary {
        let previous_path = (actual_old_path != actual_new_path).then_some(actual_old_path);
        let change_kind = resolve_change_kind(is_new, is_deleted, is_renamed, is_copied);
        return (
            Some(DiffFile {
                id: stable_file_id(&actual_new_path, previous_path.as_deref()),
                path: actual_new_path,
                previous_path,
                summary: None,
                patch: format_patch(lines),
                hunks: Vec::new(),
                change_kind,
                is_binary: true,
                is_untracked: false,
                is_too_large: false,
                stats_truncated: false,
                language: None,
                stats: FileStats::default(),
                old_source: SourceSpec::None,
                new_source: SourceSpec::None,
            }),
            file_len,
        );
    }

    // Parse --- and +++ lines (authoritative for paths).
    if i < lines.len() && lines[i].starts_with("--- ") {
        let after = &lines[i][4..]; // skip "--- "
        let path = parse_patch_path(after, "a/");
        if !path.is_empty() && path != "/dev/null" {
            actual_old_path = path;
        }
        i += 1;
    }
    if i < lines.len() && lines[i].starts_with("+++ ") {
        let after = &lines[i][4..]; // skip "+++ "
        let path = parse_patch_path(after, "b/");
        if !path.is_empty() && path != "/dev/null" {
            actual_new_path = path;
        }
        i += 1;
    }

    let mut hunks = Vec::new();
    while i < lines.len() && !lines[i].starts_with("diff --git ") {
        if lines[i].starts_with("@@ ") {
            let (hunk, consumed) = parse_hunk(&lines[i..]);
            hunks.push(hunk);
            i += consumed;
        } else {
            i += 1;
        }
    }

    let previous_path = (actual_old_path != actual_new_path).then_some(actual_old_path);
    let change_kind = resolve_change_kind(is_new, is_deleted, is_renamed, is_copied);
    let stats =
        hunks
            .iter()
            .flat_map(|hunk| &hunk.lines)
            .fold(FileStats::default(), |mut stats, line| {
                match line.kind {
                    LineType::Addition => stats.additions += 1,
                    LineType::Deletion => stats.deletions += 1,
                    LineType::Context => {}
                }
                stats
            });
    (
        Some(DiffFile {
            id: stable_file_id(&actual_new_path, previous_path.as_deref()),
            path: actual_new_path,
            previous_path,
            summary: None,
            patch: format_patch(lines),
            hunks,
            change_kind,
            is_binary,
            is_untracked: false,
            is_too_large: false,
            stats_truncated: false,
            language: None,
            stats,
            old_source: SourceSpec::None,
            new_source: SourceSpec::None,
        }),
        file_len,
    )
}

fn resolve_change_kind(
    is_new: bool,
    is_deleted: bool,
    is_renamed: bool,
    is_copied: bool,
) -> FileChangeKind {
    if is_new {
        FileChangeKind::Added
    } else if is_deleted {
        FileChangeKind::Deleted
    } else if is_copied {
        FileChangeKind::Copied
    } else if is_renamed {
        FileChangeKind::Renamed
    } else {
        FileChangeKind::Modified
    }
}

fn format_patch(lines: &[&str]) -> String {
    let mut patch = lines.join("\n");
    patch.push('\n');
    patch
}

fn parse_hunk(lines: &[&str]) -> (Hunk, usize) {
    let header = lines[0];
    let (old_start, _, new_start, _) = parse_hunk_header(header);

    let mut diff_lines = Vec::new();
    let mut old_line = old_start;
    let mut new_line = new_start;
    let mut i = 1;

    while i < lines.len() {
        let line = lines[i];
        if line.starts_with("@@ ") || line.starts_with("diff --git ") {
            break;
        }

        if let Some(content) = line.strip_prefix('+') {
            diff_lines.push(DiffLine {
                kind: LineType::Addition,
                content: content.to_string(),
                old_lineno: None,
                new_lineno: Some(new_line),
                moved: None,
            });
            new_line += 1;
        } else if let Some(content) = line.strip_prefix('-') {
            diff_lines.push(DiffLine {
                kind: LineType::Deletion,
                content: content.to_string(),
                old_lineno: Some(old_line),
                new_lineno: None,
                moved: None,
            });
            old_line += 1;
        } else if line.starts_with(' ') || line.is_empty() {
            let content = if line.is_empty() {
                String::new()
            } else {
                line[1..].to_string()
            };
            diff_lines.push(DiffLine {
                kind: LineType::Context,
                content,
                old_lineno: Some(old_line),
                new_lineno: Some(new_line),
                moved: None,
            });
            old_line += 1;
            new_line += 1;
        } else if line.starts_with('\\') {
            // "\ No newline at end of file" — skip
        } else {
            break;
        }
        i += 1;
    }

    (
        Hunk {
            old_start,
            new_start,
            header: header.to_string(),
            lines: diff_lines,
        },
        i,
    )
}

fn parse_hunk_header(header: &str) -> (u32, u32, u32, u32) {
    let header = header.trim_start_matches("@@ ");
    let parts: Vec<&str> = header.splitn(3, ' ').collect();

    let old = parts[0].trim_start_matches('-');
    let new = parts[1].trim_start_matches('+');

    let (old_start, old_count) = parse_range(old);
    let (new_start, new_count) = parse_range(new);

    (old_start, old_count, new_start, new_count)
}

/// Parse a path from a `---` or `+++` line.
/// Handles: `a/path`, `"a/path\twith\ttabs"`, `a/path\ttimestamp`, `/dev/null`
fn parse_patch_path(raw: &str, prefix: &str) -> String {
    let raw = raw.trim();
    if raw.starts_with('"') {
        // Quoted path — unquote then strip prefix
        strip_git_prefix(raw, prefix)
    } else if raw == "/dev/null" {
        "/dev/null".to_string()
    } else {
        // Unquoted — strip prefix, then remove trailing tab+metadata
        let without_prefix = raw.strip_prefix(prefix).unwrap_or(raw);
        without_prefix
            .split('\t')
            .next()
            .unwrap_or(without_prefix)
            .to_string()
    }
}

/// Unquote a git-style C-escaped path.
/// Git quotes paths containing special chars: `"a/foo\\tb.txt"` → `foo\tb.txt`
fn unquote_git_path(s: &str) -> String {
    let s = s.trim();
    // Not quoted
    if !s.starts_with('"') || !s.ends_with('"') {
        return s.to_string();
    }
    // Strip quotes
    let inner = &s[1..s.len() - 1];
    let mut bytes = Vec::with_capacity(inner.len());
    let mut chars = inner.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.peek() {
                Some('0'..='7') => {
                    // Octal escape: \NNN (1-3 octal digits)
                    let mut val: u8 = 0;
                    for _ in 0..3 {
                        if let Some(&d) = chars.peek() {
                            if ('0'..='7').contains(&d) {
                                val = val * 8 + (d as u8 - b'0');
                                chars.next();
                            } else {
                                break;
                            }
                        }
                    }
                    bytes.push(val);
                }
                _ => match chars.next() {
                    Some('n') => bytes.push(b'\n'),
                    Some('t') => bytes.push(b'\t'),
                    Some('\\') => bytes.push(b'\\'),
                    Some('"') => bytes.push(b'"'),
                    Some('a') => bytes.push(0x07),
                    Some('b') => bytes.push(0x08),
                    Some('f') => bytes.push(0x0c),
                    Some('r') => bytes.push(b'\r'),
                    Some(other) => {
                        bytes.push(b'\\');
                        let mut buf = [0u8; 4];
                        bytes.extend_from_slice(other.encode_utf8(&mut buf).as_bytes());
                    }
                    None => bytes.push(b'\\'),
                },
            }
        } else {
            let mut buf = [0u8; 4];
            bytes.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }
    // Git encodes non-ASCII as octal bytes of the UTF-8 representation
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

/// Strip the git prefix (a/ or b/) from a path, handling both quoted and unquoted forms.
fn strip_git_prefix(s: &str, prefix: &str) -> String {
    let s = s.trim();
    if s.starts_with('"') {
        // Quoted: `"a/path"` — unquote first, then strip prefix inside
        let unquoted = unquote_git_path(s);
        unquoted
            .strip_prefix(prefix)
            .unwrap_or(&unquoted)
            .to_string()
    } else {
        s.strip_prefix(prefix).unwrap_or(s).to_string()
    }
}

fn parse_range(range: &str) -> (u32, u32) {
    if let Some((start, count)) = range.split_once(',') {
        (start.parse().unwrap_or(1), count.parse().unwrap_or(0))
    } else {
        (range.parse().unwrap_or(1), 1)
    }
}

/// Find the index of the closing `"` in a C-quoted string, skipping escaped quotes.
/// Input must start with `"`. Returns the byte index of the closing quote.
fn find_closing_quote(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut i = 1;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2; // skip escaped char
        } else if bytes[i] == b'"' {
            return Some(i);
        } else {
            i += 1;
        }
    }
    None
}

fn parse_diff_git_line(line: &str) -> (String, String) {
    // "diff --git a/path b/path" or 'diff --git "a/path" "b/path"'
    let rest = line.strip_prefix("diff --git ").unwrap_or(line);

    // Quoted form: diff --git "a/..." "b/..."
    if rest.starts_with('"')
        && let Some(end) = find_closing_quote(rest)
    {
        let first = &rest[..end + 1]; // includes both quotes
        let remainder = rest[end + 1..].trim_start();
        let old = strip_git_prefix(first, "a/");
        let new = strip_git_prefix(remainder, "b/");
        return (old, new);
    }

    // Unquoted, identical paths: use midpoint symmetry
    // "a/<path> b/<path>" → total = 5 + 2*path_len
    if rest.starts_with("a/") && rest.len() >= 5 {
        let path_len = (rest.len() - 5) / 2;
        let expected_sep = 2 + path_len;
        if rest.get(expected_sep..expected_sep + 3) == Some(" b/") {
            let old = rest[2..expected_sep].to_string();
            let new = rest[expected_sep + 3..].to_string();
            return (old, new);
        }
    }

    // Fallback: split at last " b/"
    if let Some(b_idx) = rest.rfind(" b/") {
        let old = rest.get(2..b_idx).unwrap_or("").to_string();
        let new = rest[b_idx + 3..].to_string();
        (old, new)
    } else {
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        let old = parts[0].trim_start_matches("a/").to_string();
        let new = parts
            .get(1)
            .unwrap_or(&parts[0])
            .trim_start_matches("b/")
            .to_string();
        (old, new)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_diff() {
        let input = r#"diff --git a/src/main.rs b/src/main.rs
index abc1234..def5678 100644
--- a/src/main.rs
+++ b/src/main.rs
@@ -1,4 +1,5 @@
 fn main() {
-    println!("hello");
+    println!("hello world");
+    println!("goodbye");
 }
"#;
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].hunks.len(), 1);

        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.old_start, 1);
        assert_eq!(hunk.new_start, 1);
        assert_eq!(hunk.lines.len(), 5);
        assert_eq!(hunk.lines[0].kind, LineType::Context);
        assert_eq!(hunk.lines[1].kind, LineType::Deletion);
        assert_eq!(hunk.lines[2].kind, LineType::Addition);
        assert_eq!(hunk.lines[3].kind, LineType::Addition);
        assert_eq!(hunk.lines[4].kind, LineType::Context);
    }

    #[test]
    fn test_new_file() {
        let input = r#"diff --git a/new.rs b/new.rs
new file mode 100644
index 0000000..abc1234
--- /dev/null
+++ b/new.rs
@@ -0,0 +1,3 @@
+fn new_func() {
+    todo!()
+}
"#;
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].change_kind, FileChangeKind::Added);
        assert_eq!(files[0].hunks[0].lines.len(), 3);
    }

    #[test]
    fn test_deleted_file_change_kind() {
        let input = "diff --git a/old.rs b/old.rs\n\
                     deleted file mode 100644\n\
                     --- a/old.rs\n\
                     +++ /dev/null\n\
                     @@ -1 +0,0 @@\n\
                     -gone\n";
        let files = parse_unified_diff(input);
        assert_eq!(files[0].change_kind, FileChangeKind::Deleted);
    }

    #[test]
    fn test_renamed_file_has_stable_previous_path() {
        let input = "diff --git a/old.rs b/new.rs\n\
                     similarity index 100%\n\
                     rename from old.rs\n\
                     rename to new.rs\n";
        let files = parse_unified_diff(input);
        assert_eq!(files[0].change_kind, FileChangeKind::Renamed);
        assert_eq!(files[0].previous_path.as_deref(), Some("old.rs"));
        assert_eq!(files[0].id, "file:old.rs->new.rs");
    }

    #[test]
    fn test_copied_file_change_kind() {
        let input = "diff --git a/source.rs b/copy.rs\n\
                     similarity index 100%\n\
                     copy from source.rs\n\
                     copy to copy.rs\n";
        let files = parse_unified_diff(input);
        assert_eq!(files[0].change_kind, FileChangeKind::Copied);
        assert_eq!(files[0].previous_path.as_deref(), Some("source.rs"));
    }

    #[test]
    fn test_multiple_files() {
        let input = r#"diff --git a/a.rs b/a.rs
--- a/a.rs
+++ b/a.rs
@@ -1,2 +1,2 @@
-old
+new
diff --git a/b.rs b/b.rs
--- a/b.rs
+++ b/b.rs
@@ -1,2 +1,2 @@
-foo
+bar
"#;
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0].path, "a.rs");
        assert_eq!(files[1].path, "b.rs");
    }

    #[test]
    fn test_line_numbers() {
        let input = r#"diff --git a/f.rs b/f.rs
--- a/f.rs
+++ b/f.rs
@@ -10,4 +10,5 @@
 context
-deleted
+added1
+added2
 context
"#;
        let files = parse_unified_diff(input);
        let lines = &files[0].hunks[0].lines;

        assert_eq!(lines[0].old_lineno, Some(10));
        assert_eq!(lines[0].new_lineno, Some(10));
        assert_eq!(lines[1].old_lineno, Some(11));
        assert_eq!(lines[1].new_lineno, None);
        assert_eq!(lines[2].old_lineno, None);
        assert_eq!(lines[2].new_lineno, Some(11));
        assert_eq!(lines[3].old_lineno, None);
        assert_eq!(lines[3].new_lineno, Some(12));
        assert_eq!(lines[4].old_lineno, Some(12));
        assert_eq!(lines[4].new_lineno, Some(13));
    }

    #[test]
    fn test_path_with_b_slash() {
        let (old, new) = parse_diff_git_line("diff --git a/foo b/bar.txt b/foo b/bar.txt");
        assert_eq!(old, "foo b/bar.txt");
        assert_eq!(new, "foo b/bar.txt");
    }

    #[test]
    fn test_simple_path() {
        let (old, new) = parse_diff_git_line("diff --git a/src/main.rs b/src/main.rs");
        assert_eq!(old, "src/main.rs");
        assert_eq!(new, "src/main.rs");
    }

    #[test]
    fn test_quoted_path_with_tab() {
        let (old, new) = parse_diff_git_line(r#"diff --git "a/a\tb.txt" "b/a\tb.txt""#);
        assert_eq!(old, "a\tb.txt");
        assert_eq!(new, "a\tb.txt");
    }

    #[test]
    fn test_unquote_git_path() {
        assert_eq!(unquote_git_path(r#""a/foo\\bar""#), "a/foo\\bar");
        assert_eq!(unquote_git_path(r#""a/foo\tbar""#), "a/foo\tbar");
        assert_eq!(unquote_git_path("plain/path"), "plain/path");
    }

    #[test]
    fn test_unquote_octal() {
        // é is U+00E9, UTF-8 bytes \xC3\xA9, octal \303\251
        assert_eq!(unquote_git_path(r#""a/\303\251.txt""#), "a/é.txt");
    }

    #[test]
    fn test_quoted_path_with_embedded_quote() {
        // File named a"b.txt → git quotes as "a/a\"b.txt"
        let (old, new) = parse_diff_git_line(r#"diff --git "a/a\"b.txt" "b/a\"b.txt""#);
        assert_eq!(old, "a\"b.txt");
        assert_eq!(new, "a\"b.txt");
    }

    #[test]
    fn test_git_binary_patch() {
        let input = "diff --git a/img.png b/img.png\n\
                      index abc..def 100644\n\
                      GIT binary patch\n\
                      literal 1234\n\
                      zcmV;@1234abcdef\n\
                      \n\
                      literal 0\n\
                      HcmV?d00001\n\
                      \n\
                      diff --git a/other.rs b/other.rs\n\
                      --- a/other.rs\n\
                      +++ b/other.rs\n\
                      @@ -1,2 +1,2 @@\n\
                      -old\n\
                      +new\n";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 2);
        assert!(files[0].is_binary);
        assert_eq!(files[0].path, "img.png");
        assert!(!files[1].is_binary);
        assert_eq!(files[1].path, "other.rs");
    }

    #[test]
    fn test_patch_path_with_trailing_tab() {
        assert_eq!(parse_patch_path("a/file.txt\t2024-01-01", "a/"), "file.txt");
    }

    #[test]
    fn test_patch_path_quoted() {
        assert_eq!(
            parse_patch_path(r#""a/foo\tbar.txt""#, "a/"),
            "foo\tbar.txt"
        );
    }

    #[test]
    fn test_ansi_colored_diff() {
        // gh pr diff emits SGR escape sequences when GH_PAGER is set.
        // The parser must strip them before line-prefix matching.
        let input = "\x1b[1;37mdiff --git a/src/main.rs b/src/main.rs\x1b[m\n\
                     \x1b[1;37mindex abc1234..def5678 100644\x1b[m\n\
                     \x1b[1;37m--- a/src/main.rs\x1b[m\n\
                     \x1b[1;37m+++ b/src/main.rs\x1b[m\n\
                     \x1b[36m@@ -1,2 +1,2 @@\x1b[m\n\
                     \x1b[31m-old\x1b[m\n\
                     \x1b[32m+new\x1b[m\n";
        let files = parse_unified_diff(input);
        assert_eq!(files.len(), 1, "expected 1 file, got {}", files.len());
        assert_eq!(files[0].path, "src/main.rs");
        assert_eq!(files[0].hunks.len(), 1);
        let hunk = &files[0].hunks[0];
        assert_eq!(hunk.lines[0].kind, LineType::Deletion);
        assert_eq!(hunk.lines[0].content, "old");
        assert_eq!(hunk.lines[1].kind, LineType::Addition);
        assert_eq!(hunk.lines[1].content, "new");
    }
}

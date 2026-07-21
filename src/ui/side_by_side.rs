use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, Paragraph, Wrap};

use crate::app::{App, Side};
use crate::diff::model::LineType;
use crate::vim::mode::Mode;

// Purely visual separator rendered between files (not before the first file).
// Does not occupy a `flat_lines` entry, so the cursor cannot land on it.
//
// TODO (design choice): customize the look of the separator below.
// Available theme styles on `app.theme`:
//   - border        (DarkGray)
//   - file_header   (White + Bold)
//   - hunk_header   (Cyan + Bold)
// Ideas: `"─".repeat(N)`, a centered "── file N ──" banner, or a blank line.
// Returns exactly ONE rendered row (row counting in scroll math assumes this).
fn file_separator_line<'a>(_app: &'a App) -> Line<'a> {
    Line::from(Span::styled(
        "─".repeat(120),
        Style::default().fg(Color::DarkGray),
    ))
}

pub fn render(frame: &mut Frame, area: Rect, app: &mut App) {
    frame.render_widget(Clear, area);

    let chunks = Layout::vertical([
        Constraint::Min(1),
        Constraint::Length(1),
        Constraint::Length(1),
    ])
    .split(area);

    render_diff(frame, chunks[0], app);
    render_status_bar(frame, chunks[1], app);
    render_command_line(frame, chunks[2], app);

    if app.mode.is_comment() {
        render_comment_popup(frame, chunks[0], app);
    }

    if matches!(app.mode, Mode::TmuxPanePick) {
        render_tmux_pane_picker(frame, chunks[0], app);
    }
}

fn render_diff(frame: &mut Frame, area: Rect, app: &App) {
    if app.focus_mode {
        // Single panel at full width
        let lines = build_diff_lines(app, area.height as usize);
        let para = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
        frame.render_widget(para, area);
    } else if app.show_file_list {
        let flist_width = (area.width as f32 * 0.15).clamp(16.0, 30.0) as u16;
        let content_width = area.width.saturating_sub(flist_width + 2);
        let half_content = content_width / 2;

        let chunks = Layout::horizontal([
            Constraint::Length(flist_width),
            Constraint::Length(1),
            Constraint::Length(half_content),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);

        render_file_list(frame, chunks[0], app, flist_width);
        render_separator(frame, chunks[1], app);
        render_split_panels(frame, chunks[2], chunks[3], chunks[4], app);
    } else {
        let half = area.width / 2;
        let chunks = Layout::horizontal([
            Constraint::Length(half),
            Constraint::Length(1),
            Constraint::Min(1),
        ])
        .split(area);

        render_split_panels(frame, chunks[0], chunks[1], chunks[2], app);
    }
}

fn render_file_list(frame: &mut Frame, area: Rect, app: &App, width: u16) {
    let active_file = app.active_file_idx();
    let counts = &app.line_counts;
    let max_w = width as usize;

    let mut lines = Vec::new();

    let total_adds: usize = counts.iter().map(|(a, _)| a).sum();
    let total_dels: usize = counts.iter().map(|(_, d)| d).sum();
    lines.push(Line::from(vec![
        Span::styled(format!("{} files ", app.files.len()), app.theme.file_header),
        Span::styled(
            format!("+{total_adds}"),
            app.theme.line_style(&LineType::Addition),
        ),
        Span::styled(" ", Style::default()),
        Span::styled(
            format!("-{total_dels}"),
            app.theme.line_style(&LineType::Deletion),
        ),
    ]));
    lines.push(Line::default());

    for (i, file) in app.files.iter().enumerate() {
        let is_active = active_file == Some(i);
        let (adds, dels) = counts[i];
        let style = if is_active {
            app.theme.file_list_active
        } else {
            app.theme.file_list_item
        };
        let display_name = short_filename(&file.path, max_w.saturating_sub(8));
        let marker = if is_active { "▶ " } else { "  " };

        let mut spans = vec![
            Span::styled(marker, style),
            Span::styled(display_name, style),
        ];
        if adds > 0 {
            spans.push(Span::styled(
                format!(" +{adds}"),
                app.theme.line_style(&LineType::Addition),
            ));
        }
        if dels > 0 {
            spans.push(Span::styled(
                format!(" -{dels}"),
                app.theme.line_style(&LineType::Deletion),
            ));
        }
        lines.push(Line::from(spans));
    }

    let para = Paragraph::new(lines).block(Block::default().borders(Borders::NONE));
    frame.render_widget(para, area);
}

fn render_separator(frame: &mut Frame, area: Rect, app: &App) {
    let sep: Vec<Line> = (0..area.height)
        .map(|_| Line::from(Span::styled("│", app.theme.border)))
        .collect();
    frame.render_widget(Paragraph::new(sep), area);
}

/// Build the visible diff lines for a viewport. When `single` is true, produces
/// one set of lines (focus mode). When false, produces paired (left, right) lines.
fn build_diff_lines<'a>(app: &'a App, viewport_height: usize) -> Vec<Line<'a>> {
    let selection = app.selection_range();
    let is_left = app.focus_side == Side::Left;
    let mut lines = Vec::with_capacity(viewport_height);
    let mut last_file: Option<usize> = None;
    let mut last_hunk: Option<(usize, usize)> = None;
    let mut flat_idx = app.scroll_offset;
    let mut rows = 0usize;

    while flat_idx < app.flat_lines.len() && rows < viewport_height {
        let fl = &app.flat_lines[flat_idx];
        let diff_line = match app.get_line(flat_idx) {
            Some(l) => l,
            None => break,
        };

        let is_cursor = flat_idx == app.cursor;
        let is_selected = selection.is_some_and(|(s, e)| flat_idx >= s && flat_idx <= e);

        if last_file != Some(fl.file_idx) {
            if last_file.is_some() {
                if rows >= viewport_height {
                    break;
                }
                lines.push(file_separator_line(app));
                rows += 1;
            }
            if rows >= viewport_height {
                break;
            }
            let file = &app.files[fl.file_idx];
            let header = match &file.previous_path {
                Some(old) => format!(" {} → {}", old, file.path),
                None => format!(" {}", file.path),
            };
            lines.push(Line::from(Span::styled(header, app.theme.file_header)));
            rows += 1;
            last_file = Some(fl.file_idx);
            last_hunk = None;
        }

        if last_hunk != Some((fl.file_idx, fl.hunk_idx)) && fl.line_idx == 0 {
            if rows >= viewport_height {
                break;
            }
            let hunk = &app.files[fl.file_idx].hunks[fl.hunk_idx];
            lines.push(Line::from(Span::styled(
                hunk.header.clone(),
                app.theme.hunk_header,
            )));
            rows += 1;
            last_hunk = Some((fl.file_idx, fl.hunk_idx));
        }

        if rows >= viewport_height {
            break;
        }

        // In focus mode, skip lines that don't belong to the focused side
        if app.line_hidden_on_side(diff_line) {
            flat_idx += 1;
            continue;
        }

        let line_style = app.theme.line_style(&diff_line.kind);
        let lineno_style = app.theme.lineno_style(&diff_line.kind);
        let lineno = if is_left {
            diff_line.old_lineno
        } else {
            diff_line.new_lineno
        };

        let annotation = app
            .annotations
            .iter()
            .find(|a| flat_idx >= a.flat_start && flat_idx <= a.flat_end);
        let (marker, marker_style) = if annotation.is_some() {
            ("● ", app.theme.comment_indicator)
        } else {
            ("  ", Style::default())
        };

        let content = build_content_spans(
            &diff_line.content,
            fl.file_idx,
            fl.hunk_idx,
            fl.line_idx,
            line_style,
            is_cursor,
            is_selected,
            &app.theme,
            &app.highlighter,
        );

        let mut spans = vec![
            Span::styled(format_lineno(lineno), lineno_style),
            Span::styled(marker, marker_style),
        ];
        spans.extend(content);
        lines.push(Line::from(spans));
        rows += 1;

        if let Some(ann) = annotation
            && app.show_comments
            && flat_idx == ann.flat_end
        {
            for cl in ann.comment.lines() {
                if rows >= viewport_height {
                    break;
                }
                lines.push(Line::from(vec![
                    Span::styled("     ", Style::default()),
                    Span::styled(format!("# {cl}"), app.theme.comment_indicator),
                ]));
                rows += 1;
            }
        }

        flat_idx += 1;
    }

    lines
}

fn render_split_panels(
    frame: &mut Frame,
    left_area: Rect,
    sep_area: Rect,
    right_area: Rect,
    app: &App,
) {
    let viewport_height = left_area.height as usize;
    let selection = app.selection_range();

    let mut left_lines = Vec::with_capacity(viewport_height);
    let mut right_lines = Vec::with_capacity(viewport_height);

    let mut last_file: Option<usize> = None;
    let mut last_hunk: Option<(usize, usize)> = None;
    let mut flat_idx = app.scroll_offset;
    let mut rows = 0usize;

    while flat_idx < app.flat_lines.len() && rows < viewport_height {
        let fl = &app.flat_lines[flat_idx];
        let diff_line = match app.get_line(flat_idx) {
            Some(l) => l,
            None => break,
        };

        let is_cursor = flat_idx == app.cursor;
        let is_selected = selection.is_some_and(|(s, e)| flat_idx >= s && flat_idx <= e);

        if last_file != Some(fl.file_idx) {
            if last_file.is_some() {
                if rows >= viewport_height {
                    break;
                }
                left_lines.push(file_separator_line(app));
                right_lines.push(file_separator_line(app));
                rows += 1;
            }
            if rows >= viewport_height {
                break;
            }
            let file = &app.files[fl.file_idx];
            let header = match &file.previous_path {
                Some(old) => format!(" {} → {}", old, file.path),
                None => format!(" {}", file.path),
            };
            left_lines.push(Line::from(Span::styled(
                header.clone(),
                app.theme.file_header,
            )));
            right_lines.push(Line::from(Span::styled(header, app.theme.file_header)));
            rows += 1;
            last_file = Some(fl.file_idx);
            last_hunk = None;
        }

        if last_hunk != Some((fl.file_idx, fl.hunk_idx)) && fl.line_idx == 0 {
            if rows >= viewport_height {
                break;
            }
            let hunk = &app.files[fl.file_idx].hunks[fl.hunk_idx];
            left_lines.push(Line::from(Span::styled(
                hunk.header.clone(),
                app.theme.hunk_header,
            )));
            right_lines.push(Line::from(Span::styled(
                hunk.header.clone(),
                app.theme.hunk_header,
            )));
            rows += 1;
            last_hunk = Some((fl.file_idx, fl.hunk_idx));
        }

        if rows >= viewport_height {
            break;
        }

        let line_style = app.theme.line_style(&diff_line.kind);
        let lineno_style = app.theme.lineno_style(&diff_line.kind);

        let annotation = app
            .annotations
            .iter()
            .find(|a| flat_idx >= a.flat_start && flat_idx <= a.flat_end);
        let (marker, marker_style) = if annotation.is_some() {
            ("● ", app.theme.comment_indicator)
        } else {
            ("  ", Style::default())
        };

        let left_is_cursor = is_cursor && app.focus_side == Side::Left;
        let right_is_cursor = is_cursor && app.focus_side == Side::Right;

        // Only build content twice when cursor is on this line (different styling per side)
        if is_cursor {
            let left_content = build_content_spans(
                &diff_line.content,
                fl.file_idx,
                fl.hunk_idx,
                fl.line_idx,
                line_style,
                left_is_cursor,
                is_selected,
                &app.theme,
                &app.highlighter,
            );
            let right_content = build_content_spans(
                &diff_line.content,
                fl.file_idx,
                fl.hunk_idx,
                fl.line_idx,
                line_style,
                right_is_cursor,
                is_selected,
                &app.theme,
                &app.highlighter,
            );
            push_diff_line(
                &diff_line.kind,
                diff_line,
                lineno_style,
                marker,
                marker_style,
                left_content,
                right_content,
                &mut left_lines,
                &mut right_lines,
            );
        } else {
            let content = build_content_spans(
                &diff_line.content,
                fl.file_idx,
                fl.hunk_idx,
                fl.line_idx,
                line_style,
                false,
                is_selected,
                &app.theme,
                &app.highlighter,
            );
            push_diff_line(
                &diff_line.kind,
                diff_line,
                lineno_style,
                marker,
                marker_style,
                content.clone(),
                content,
                &mut left_lines,
                &mut right_lines,
            );
        }
        rows += 1;

        if let Some(ann) = annotation
            && app.show_comments
            && flat_idx == ann.flat_end
        {
            for cl in ann.comment.lines() {
                if rows >= viewport_height {
                    break;
                }
                let comment_span = Line::from(vec![
                    Span::styled("     ", Style::default()),
                    Span::styled(format!("# {cl}"), app.theme.comment_indicator),
                ]);
                // Show comment on the side matching the line type
                let on_left = diff_line.kind == LineType::Deletion
                    || (diff_line.kind == LineType::Context && app.focus_side == Side::Left);
                if on_left {
                    left_lines.push(comment_span);
                    right_lines.push(Line::default());
                } else {
                    left_lines.push(Line::default());
                    right_lines.push(comment_span);
                }
                rows += 1;
            }
        }

        flat_idx += 1;
    }

    let lw = left_area.width;
    let rw = right_area.width;
    let left_t: Vec<Line> = left_lines
        .into_iter()
        .map(|l| truncate_line(l, lw))
        .collect();
    let right_t: Vec<Line> = right_lines
        .into_iter()
        .map(|l| truncate_line(l, rw))
        .collect();

    frame.render_widget(
        Paragraph::new(left_t).block(Block::default().borders(Borders::NONE)),
        left_area,
    );
    render_separator(frame, sep_area, app);
    frame.render_widget(
        Paragraph::new(right_t).block(Block::default().borders(Borders::NONE)),
        right_area,
    );
}

// The renderer keeps paired-panel state explicit until the UI parity slice
// replaces this legacy function with a deeper rendering module.
#[allow(clippy::too_many_arguments)]
fn push_diff_line<'a>(
    kind: &LineType,
    diff_line: &crate::diff::model::DiffLine,
    lineno_style: Style,
    marker: &'a str,
    marker_style: Style,
    left_content: Vec<Span<'a>>,
    right_content: Vec<Span<'a>>,
    left_lines: &mut Vec<Line<'a>>,
    right_lines: &mut Vec<Line<'a>>,
) {
    match kind {
        LineType::Context => {
            let old_no = format_lineno(diff_line.old_lineno);
            let new_no = format_lineno(diff_line.new_lineno);
            let mut ls = vec![
                Span::styled(old_no, lineno_style),
                Span::styled(marker, marker_style),
            ];
            ls.extend(left_content);
            left_lines.push(Line::from(ls));
            let mut rs = vec![
                Span::styled(new_no, lineno_style),
                Span::styled("  ", Style::default()),
            ];
            rs.extend(right_content);
            right_lines.push(Line::from(rs));
        }
        LineType::Deletion => {
            let old_no = format_lineno(diff_line.old_lineno);
            let mut ls = vec![
                Span::styled(old_no, lineno_style),
                Span::styled(marker, marker_style),
            ];
            ls.extend(left_content);
            left_lines.push(Line::from(ls));
            right_lines.push(Line::default());
        }
        LineType::Addition => {
            let new_no = format_lineno(diff_line.new_lineno);
            left_lines.push(Line::default());
            let mut rs = vec![
                Span::styled(new_no, lineno_style),
                Span::styled(marker, marker_style),
            ];
            rs.extend(right_content);
            right_lines.push(Line::from(rs));
        }
    }
}

fn truncate_line(line: Line<'_>, max_width: u16) -> Line<'_> {
    let max = max_width as usize;
    let mut width = 0usize;
    let mut new_spans = Vec::new();

    for span in line.spans {
        let span_chars: usize = span.content.chars().count();
        if width + span_chars <= max {
            width += span_chars;
            new_spans.push(span);
        } else {
            let remaining = max.saturating_sub(width);
            if remaining > 1 {
                let truncated: String = span.content.chars().take(remaining - 1).collect();
                new_spans.push(Span::styled(format!("{truncated}…"), span.style));
            } else if remaining == 1 {
                new_spans.push(Span::styled("…", span.style));
            }
            break;
        }
    }

    Line::from(new_spans)
}

fn render_tmux_pane_picker(frame: &mut Frame, diff_area: Rect, app: &App) {
    let popup_width = (diff_area.width as f32 * 0.6).clamp(40.0, 100.0) as u16;
    let list_rows = app.tmux_panes.len() as u16;
    let desired_height = list_rows.saturating_add(3); // border + hint + padding
    let popup_height = desired_height.min(diff_area.height).max(5);

    if popup_height == 0 || diff_area.width < 10 {
        return;
    }

    let popup_x = diff_area.x + (diff_area.width.saturating_sub(popup_width)) / 2;
    let popup_y = diff_area.y + (diff_area.height.saturating_sub(popup_height)) / 2;
    let popup_rect = Rect::new(
        popup_x,
        popup_y,
        popup_width.min(diff_area.width),
        popup_height,
    );

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Magenta))
        .title(Span::styled(
            " TMUX ",
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(popup_rect);
    let inner_width = inner.width as usize;

    let mut lines: Vec<Line> = Vec::with_capacity(app.tmux_panes.len() + 1);
    for (i, pane) in app.tmux_panes.iter().enumerate() {
        let selected = i == app.tmux_cursor;
        let prefix = if selected { "▶ " } else { "  " };
        let row = format!("{}{}", prefix, pane.label);
        let truncated: String = row.chars().take(inner_width).collect();
        let style = if selected {
            Style::default()
                .fg(Color::Black)
                .bg(Color::Magenta)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
        };
        lines.push(Line::from(Span::styled(truncated, style)));
    }
    lines.push(Line::default());
    lines.push(Line::from(Span::styled(
        "j/k:move  Enter:send  Esc:cancel",
        Style::default().fg(Color::DarkGray),
    )));

    let content = Paragraph::new(lines);

    frame.render_widget(Clear, popup_rect);
    frame.render_widget(block, popup_rect);
    frame.render_widget(content, inner);
}

fn render_comment_popup(frame: &mut Frame, diff_area: Rect, app: &App) {
    let cursor_screen_row = app
        .rendered_rows_between(app.scroll_offset, app.cursor)
        .min(diff_area.height as usize);

    let popup_width = (diff_area.width as f32 * 0.5).clamp(30.0, 60.0) as u16;
    let inner_width = popup_width.saturating_sub(2) as usize;

    // Count rendered rows given hard newlines AND soft wrap.
    let text_rows = if inner_width == 0 {
        1
    } else {
        app.comment_buf
            .split('\n')
            .map(|line| line.chars().count().div_ceil(inner_width).max(1))
            .sum::<usize>()
            .max(1)
    };

    // Layout inside the bordered block: text + blank separator + hint line.
    // Add 2 for top/bottom borders. Min 5 so the hint is always visible.
    let desired_height = (text_rows as u16 + 4).max(5);
    let popup_height = desired_height.min(diff_area.height);

    if popup_height == 0 || diff_area.width < 10 {
        return;
    }

    let popup_y = if cursor_screen_row as u16 + popup_height + 2 < diff_area.height {
        diff_area.y + cursor_screen_row as u16 + 1
    } else {
        diff_area
            .y
            .saturating_add(cursor_screen_row as u16)
            .saturating_sub(popup_height + 1)
    };

    let popup_x = diff_area.x + (diff_area.width.saturating_sub(popup_width)) / 2;
    let popup_rect = Rect::new(
        popup_x,
        popup_y.min(diff_area.y + diff_area.height.saturating_sub(popup_height)),
        popup_width.min(diff_area.width),
        popup_height,
    );

    let (mode_label, cursor_char, hint) = match &app.mode {
        Mode::CommentInsert => (
            " INSERT ",
            "█",
            "Enter:submit  ^T:send+save to tmux  Esc:normal",
        ),
        Mode::CommentNormal => (" NORMAL ", "▋", "a/i:insert  Enter/c:submit  Esc:cancel"),
        _ => ("", "", ""),
    };

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Yellow))
        .title(Span::styled(
            mode_label,
            Style::default()
                .fg(Color::Black)
                .bg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ));

    let inner = block.inner(popup_rect);

    // Split buf on '\n' so Line widgets wrap cleanly; cursor glyph on the last.
    let parts: Vec<&str> = if app.comment_buf.is_empty() {
        vec![""]
    } else {
        app.comment_buf.split('\n').collect()
    };
    let mut content_lines: Vec<Line> = Vec::with_capacity(parts.len() + 2);
    for (i, part) in parts.iter().enumerate() {
        let mut spans = vec![Span::raw(part.to_string())];
        if i + 1 == parts.len() {
            spans.push(Span::styled(
                cursor_char,
                Style::default().fg(Color::Yellow),
            ));
        }
        content_lines.push(Line::from(spans));
    }
    content_lines.push(Line::default());
    content_lines.push(Line::from(Span::styled(
        hint,
        Style::default().fg(Color::DarkGray),
    )));

    let content = Paragraph::new(content_lines).wrap(Wrap { trim: false });

    frame.render_widget(Clear, popup_rect);
    frame.render_widget(block, popup_rect);
    frame.render_widget(content, inner);
}

fn short_filename(path: &str, max_width: usize) -> String {
    let char_count = path.chars().count();
    if char_count <= max_width {
        return path.to_string();
    }
    if let Some(name) = path.rsplit('/').next() {
        let name_chars = name.chars().count();
        if name_chars >= max_width.saturating_sub(1) {
            let skip = name_chars.saturating_sub(max_width - 1);
            format!("…{}", name.chars().skip(skip).collect::<String>())
        } else {
            format!("…/{name}")
        }
    } else {
        path.chars().take(max_width).collect()
    }
}

fn render_status_bar(frame: &mut Frame, area: Rect, app: &App) {
    let mode_style = app.theme.mode_style(&app.mode);

    let file_info = app
        .flat_lines
        .get(app.cursor)
        .map(|fl| app.files[fl.file_idx].path.as_str())
        .unwrap_or("");

    let side_label = match (app.focus_mode, app.focus_side) {
        (true, Side::Left) => "[OLD FOCUS]",
        (true, Side::Right) => "[NEW FOCUS]",
        (false, Side::Left) => "[OLD]",
        (false, Side::Right) => "[NEW]",
    };

    let annotations_count = if app.annotations.is_empty() {
        String::new()
    } else {
        format!(" [{}]", app.annotations.len())
    };

    let search_info = if !app.search_query.is_empty() && !app.search_matches.is_empty() {
        let pos = app.search_matches.partition_point(|&m| m <= app.cursor);
        format!(
            " /{} ({}/{})",
            app.search_query,
            pos,
            app.search_matches.len()
        )
    } else {
        String::new()
    };

    let toast = app
        .toast
        .as_deref()
        .map(|t| format!(" {t} "))
        .unwrap_or_default();

    let bar = Line::from(vec![
        Span::styled(format!(" {} ", app.mode.label()), mode_style),
        Span::styled(format!(" {side_label} "), Style::default().fg(Color::Cyan)),
        Span::styled(format!(" {file_info} "), app.theme.status_bar),
        Span::styled(annotations_count, app.theme.comment_indicator),
        Span::styled(search_info, app.theme.status_bar),
        Span::styled(
            toast,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        ),
        Span::styled(
            format!(" {}/{} ", app.cursor + 1, app.flat_lines.len()),
            app.theme.status_bar,
        ),
    ]);

    frame.render_widget(Paragraph::new(bar), area);
}

fn render_command_line(frame: &mut Frame, area: Rect, app: &App) {
    let content = match &app.mode {
        Mode::Command => Line::from(vec![
            Span::raw("/"),
            Span::raw(&app.search_query),
            Span::styled("█", Style::default()),
        ]),
        _ => {
            let hints = match &app.mode {
                Mode::Normal => {
                    "q:quit  V:visual  yy:yank  ^T:tmux  c:comment  /:search  ]/[:hunk  H/L:file  h/l:side  e:filelist  E:comments  F:focus"
                }
                Mode::VisualLine { .. } => "y:yank  ^T:tmux  c:comment  Esc:cancel  j/k:extend",
                Mode::TmuxPanePick => "j/k:move  Enter:send  Esc:cancel",
                _ => "",
            };
            Line::from(Span::styled(hints, app.theme.border))
        }
    };
    frame.render_widget(Paragraph::new(content), area);
}

// The highlighter lookup coordinates mirror the legacy diff-line model. They
// stay separate until the UI parity slice introduces a rendered-line context.
#[allow(clippy::too_many_arguments)]
fn build_content_spans(
    content: &str,
    file_idx: usize,
    hunk_idx: usize,
    line_idx: usize,
    line_style: Style,
    is_cursor: bool,
    is_selected: bool,
    theme: &crate::ui::theme::Theme,
    highlighter: &crate::ui::highlight::Highlighter,
) -> Vec<Span<'static>> {
    if is_cursor {
        vec![Span::styled(
            content.to_string(),
            line_style.add_modifier(Modifier::REVERSED),
        )]
    } else if is_selected {
        vec![Span::styled(content.to_string(), theme.selection)]
    } else {
        let hl_spans = highlighter.get_spans(file_idx, hunk_idx, line_idx);
        if hl_spans.is_empty() {
            vec![Span::styled(content.to_string(), line_style)]
        } else {
            hl_spans
                .into_iter()
                .map(|span| {
                    let mut style = span.style;
                    if let Some(bg) = line_style.bg {
                        style = style.bg(bg);
                    }
                    Span::styled(span.content.into_owned(), style)
                })
                .collect()
        }
    }
}

fn format_lineno(lineno: Option<u32>) -> String {
    match lineno {
        Some(n) => format!("{n:>4} "),
        None => "     ".to_string(),
    }
}

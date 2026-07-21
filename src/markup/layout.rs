use std::collections::VecDeque;
use std::sync::{Arc, Mutex, OnceLock};

use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use super::{StmlElement, StmlNode, StmlParseLimits, decode_stml_entities, parse_stml};

pub const MIN_STML_LAYOUT_WIDTH: u16 = 8;
pub const STML_REFERENCE_WIDTH: u16 = 56;

const MAX_LAYOUT_ERRORS: usize = 20;
const LAYOUT_CACHE_ITEMS: usize = 128;
const LAYOUT_CACHE_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StmlStyle {
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub dim: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StmlSpan {
    pub text: String,
    pub fg: Option<String>,
    pub bg: Option<String>,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strike: bool,
    pub dim: bool,
}

impl StmlStyle {
    fn span(&self, text: impl Into<String>) -> StmlSpan {
        StmlSpan {
            text: text.into(),
            fg: self.fg.clone(),
            bg: self.bg.clone(),
            bold: self.bold,
            italic: self.italic,
            underline: self.underline,
            strike: self.strike,
            dim: self.dim,
        }
    }

    fn merge_attrs(&self, element: &StmlElement) -> Self {
        let mut style = self.clone();
        if let Some(value) = element
            .attrs
            .get("fg")
            .or_else(|| element.attrs.get("color"))
        {
            style.fg = Some(value.clone());
        }
        if let Some(value) = element.attrs.get("bg") {
            style.bg = Some(value.clone());
        }
        for (name, target) in [
            ("bold", &mut style.bold),
            ("italic", &mut style.italic),
            ("underline", &mut style.underline),
            ("strike", &mut style.strike),
            ("dim", &mut style.dim),
        ] {
            if let Some(value) = element.attrs.get(name) {
                *target = truthy(value);
            }
        }
        style
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StmlLine {
    pub spans: Vec<StmlSpan>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StmlLayoutResult {
    pub lines: Vec<StmlLine>,
    pub errors: Vec<String>,
}

pub fn layout_stml(markup: &str, width: u16) -> StmlLayoutResult {
    if width < MIN_STML_LAYOUT_WIDTH {
        return StmlLayoutResult {
            lines: Vec::new(),
            errors: vec![format!(
                "width {width} below minimum {MIN_STML_LAYOUT_WIDTH}"
            )],
        };
    }
    let parsed = parse_stml(markup, StmlParseLimits::default());
    let mut layout = Layout {
        errors: parsed.errors.into_iter().take(MAX_LAYOUT_ERRORS).collect(),
    };
    let mut lines = layout.nodes(&parsed.nodes, usize::from(width), &StmlStyle::default());
    while lines.first().is_some_and(blank_line) {
        lines.remove(0);
    }
    while lines.last().is_some_and(blank_line) {
        lines.pop();
    }
    StmlLayoutResult {
        lines,
        errors: layout.errors,
    }
}

#[derive(Default)]
struct LayoutCache {
    entries: VecDeque<((u16, String), Arc<StmlLayoutResult>, usize)>,
    bytes: usize,
}

static LAYOUT_CACHE: OnceLock<Mutex<LayoutCache>> = OnceLock::new();

pub fn layout_stml_cached(markup: &str, width: u16) -> Arc<StmlLayoutResult> {
    let cache = LAYOUT_CACHE.get_or_init(|| Mutex::new(LayoutCache::default()));
    let mut cache = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(index) =
        cache
            .entries
            .iter()
            .position(|((candidate_width, candidate_markup), _, _)| {
                *candidate_width == width && candidate_markup == markup
            })
    {
        let entry = cache.entries.remove(index).expect("cache entry exists");
        let result = Arc::clone(&entry.1);
        cache.entries.push_back(entry);
        return result;
    }

    let result = Arc::new(layout_stml(markup, width));
    let cost = markup.len().saturating_add(
        result
            .lines
            .iter()
            .flat_map(|line| &line.spans)
            .map(|span| span.text.len())
            .sum::<usize>(),
    );
    while !cache.entries.is_empty()
        && (cache.entries.len() >= LAYOUT_CACHE_ITEMS
            || cache.bytes.saturating_add(cost) > LAYOUT_CACHE_BYTES)
    {
        if let Some((_, _, removed)) = cache.entries.pop_front() {
            cache.bytes = cache.bytes.saturating_sub(removed);
        }
    }
    cache.bytes = cache.bytes.saturating_add(cost);
    cache
        .entries
        .push_back(((width, markup.to_owned()), Arc::clone(&result), cost));
    result
}

struct Layout {
    errors: Vec<String>,
}

impl Layout {
    fn error(&mut self, message: impl Into<String>) {
        if self.errors.len() < MAX_LAYOUT_ERRORS {
            self.errors.push(message.into());
        } else if self.errors.len() == MAX_LAYOUT_ERRORS {
            self.errors.push("further layout notes omitted".into());
        }
    }

    fn nodes(&mut self, nodes: &[StmlNode], width: usize, style: &StmlStyle) -> Vec<StmlLine> {
        let mut output = Vec::new();
        let mut inline = Vec::new();
        for node in nodes {
            if is_inline(node) {
                self.inline(node, style, &mut inline);
            } else {
                self.flush_inline(&mut output, &mut inline, width);
                output.extend(self.block(node, width, style));
            }
        }
        self.flush_inline(&mut output, &mut inline, width);
        output
    }

    fn flush_inline(
        &mut self,
        output: &mut Vec<StmlLine>,
        spans: &mut Vec<StmlSpan>,
        width: usize,
    ) {
        if spans
            .iter()
            .any(|span| !span.text.trim().is_empty() || span.text == "\n")
        {
            output.extend(wrap_spans(std::mem::take(spans), width));
        } else {
            spans.clear();
        }
    }

    fn inline(&mut self, node: &StmlNode, style: &StmlStyle, spans: &mut Vec<StmlSpan>) {
        match node {
            StmlNode::Text(text) => push_span(
                spans,
                style.span(collapse_whitespace(&decode_stml_entities(text))),
            ),
            StmlNode::Element(element) if element.tag == "br" => {
                push_span(spans, style.span("\n"));
            }
            StmlNode::Element(element) => {
                let mut next = style.clone();
                match element.tag.as_str() {
                    "b" | "strong" => next.bold = true,
                    "i" | "em" => next.italic = true,
                    "u" => next.underline = true,
                    "s" | "strike" | "del" => next.strike = true,
                    "dim" | "muted" => next.dim = true,
                    "a" | "link" => {
                        next.fg = Some("accent".into());
                        next.underline = true;
                    }
                    "kbd" => {
                        next.fg = Some("heading".into());
                        next.bg = Some("subtle".into());
                    }
                    "badge" => {
                        next.fg = Some(
                            element
                                .attrs
                                .get("fg")
                                .cloned()
                                .unwrap_or_else(|| "badge-text".into()),
                        );
                        next.bg = Some(
                            element
                                .attrs
                                .get("color")
                                .or_else(|| element.attrs.get("bg"))
                                .cloned()
                                .unwrap_or_else(|| "accent".into()),
                        );
                        next.bold = true;
                    }
                    "c" | "color" | "span" => next = next.merge_attrs(element),
                    tag => self.error(format!("unknown tag <{tag}>")),
                }
                let padded = matches!(element.tag.as_str(), "kbd" | "badge");
                if padded {
                    push_span(spans, next.span(" "));
                }
                for child in &element.children {
                    self.inline(child, &next, spans);
                }
                if padded {
                    push_span(spans, next.span(" "));
                }
            }
        }
    }

    fn block(&mut self, node: &StmlNode, width: usize, style: &StmlStyle) -> Vec<StmlLine> {
        let StmlNode::Element(element) = node else {
            return Vec::new();
        };
        match element.tag.as_str() {
            "box" | "card" | "col" | "column" | "stack" | "section" => {
                self.boxed(element, width, style)
            }
            "row" => self.row(element, width, style),
            "text" | "p" => {
                let child_style = style.merge_attrs(element);
                self.nodes(&element.children, width, &child_style)
            }
            "h" | "h1" | "h2" | "h3" | "heading" | "title" => {
                let mut heading = style.clone();
                heading.bold = true;
                heading.fg = Some(
                    element
                        .attrs
                        .get("fg")
                        .or_else(|| element.attrs.get("color"))
                        .cloned()
                        .unwrap_or_else(|| "heading".into()),
                );
                heading.underline = matches!(element.tag.as_str(), "h1" | "title");
                self.nodes(&element.children, width, &heading)
            }
            "list" | "ul" | "ol" => self.list(element, width, style),
            "item" | "li" => self.list_item("• ", element, width, style),
            "code" | "pre" => self.code(element, width, style),
            "hr" | "rule" | "divider" => vec![StmlLine {
                spans: vec![
                    StmlStyle {
                        fg: Some(
                            element
                                .attrs
                                .get("color")
                                .cloned()
                                .unwrap_or_else(|| "muted".into()),
                        ),
                        ..StmlStyle::default()
                    }
                    .span("─".repeat(width)),
                ],
            }],
            "spacer" | "space" => {
                let count = numeric_attr(element, "size").unwrap_or(1).clamp(1, 20);
                vec![StmlLine::default(); count]
            }
            tag => {
                self.error(format!("unknown tag <{tag}>"));
                self.nodes(&element.children, width, style)
            }
        }
    }

    fn boxed(&mut self, element: &StmlElement, width: usize, style: &StmlStyle) -> Vec<StmlLine> {
        let card = element.tag == "card";
        let bordered = element.attrs.get("border").map_or(
            card || element.attrs.contains_key("border-style"),
            |value| truthy(value),
        );
        let border_style = element
            .attrs
            .get("border-style")
            .map(String::as_str)
            .unwrap_or(if card { "rounded" } else { "single" });
        let chars = match BorderChars::from_name(border_style) {
            Some(chars) => chars,
            None => {
                self.error(format!("unknown border-style \"{border_style}\""));
                BorderChars::single()
            }
        };
        let padding = numeric_attr(element, "padding").unwrap_or(usize::from(card));
        let padding_x = numeric_attr(element, "padding-x").unwrap_or(padding);
        let padding_y = numeric_attr(element, "padding-y").unwrap_or(padding);
        let box_width = width_attr(element.attrs.get("width"), width)
            .unwrap_or(width)
            .clamp(4.min(width), width);
        let frame = usize::from(bordered) * 2;
        let padding_x = padding_x.min(box_width.saturating_sub(frame + 1) / 2);
        let inner_width = box_width
            .saturating_sub(frame + padding_x.saturating_mul(2))
            .max(1);
        let child_style = style.merge_attrs(element);
        let content = self.nodes(&element.children, inner_width, &child_style);
        frame_lines(
            content,
            FrameOptions {
                width: box_width,
                bordered,
                chars,
                border_color: element
                    .attrs
                    .get("border-color")
                    .cloned()
                    .unwrap_or_else(|| "note-border".into()),
                title: element.attrs.get("title").cloned(),
                title_color: element
                    .attrs
                    .get("title-color")
                    .cloned()
                    .unwrap_or_else(|| "heading".into()),
                background: element.attrs.get("bg").cloned(),
                padding_x,
                padding_y,
            },
        )
    }

    fn row(&mut self, element: &StmlElement, width: usize, style: &StmlStyle) -> Vec<StmlLine> {
        let children: Vec<_> = element
            .children
            .iter()
            .filter_map(|node| match node {
                StmlNode::Element(child) if !is_inline(node) => Some(child),
                _ => None,
            })
            .collect();
        if children.is_empty() {
            return self.nodes(&element.children, width, style);
        }
        let loose: Vec<_> = element
            .children
            .iter()
            .filter(|node| is_inline(node))
            .cloned()
            .collect();
        let meaningful_loose = loose.iter().any(|node| match node {
            StmlNode::Text(text) => !text.trim().is_empty(),
            StmlNode::Element(_) => true,
        });
        let mut output = if meaningful_loose {
            self.error("<row> mixes bare text with block children; text laid out above the row");
            self.nodes(&loose, width, style)
        } else {
            Vec::new()
        };
        let gap = numeric_attr(element, "gap").unwrap_or(1).min(width);
        let total_gap = gap.saturating_mul(children.len().saturating_sub(1));
        let available = width.saturating_sub(total_gap);
        if available < children.len() {
            self.error("<row> too narrow for its columns; stacking vertically");
            output.extend(
                children
                    .into_iter()
                    .flat_map(|child| self.block(&StmlNode::Element(child.clone()), width, style)),
            );
            return output;
        }

        let fixed: Vec<_> = children
            .iter()
            .map(|child| width_attr(child.attrs.get("width"), available))
            .collect();
        let fixed_total: usize = fixed.iter().flatten().sum();
        let flex_count = fixed.iter().filter(|value| value.is_none()).count();
        let flex_space = available.saturating_sub(fixed_total).max(flex_count);
        let flex_width = if flex_count == 0 {
            0
        } else {
            flex_space / flex_count
        };
        let mut remainder = if flex_count == 0 {
            0
        } else {
            flex_space - flex_width * flex_count
        };
        let widths: Vec<_> = fixed
            .into_iter()
            .map(|fixed| {
                fixed.map_or_else(
                    || {
                        let extra = usize::from(remainder > 0);
                        remainder = remainder.saturating_sub(extra);
                        (flex_width + extra).max(1)
                    },
                    |fixed| fixed.clamp(1, available),
                )
            })
            .collect();
        let columns: Vec<_> = children
            .into_iter()
            .zip(&widths)
            .map(|(child, column_width)| {
                self.block(&StmlNode::Element(child.clone()), *column_width, style)
            })
            .collect();
        output.extend(merge_columns(columns, &widths, gap));
        output
    }

    fn list(&mut self, element: &StmlElement, width: usize, style: &StmlStyle) -> Vec<StmlLine> {
        let ordered = element.tag == "ol";
        let marker = element
            .attrs
            .get("marker")
            .cloned()
            .unwrap_or_else(|| "•".into());
        let mut output = Vec::new();
        let mut index = 1;
        for child in &element.children {
            let StmlNode::Element(item) = child else {
                continue;
            };
            if !matches!(item.tag.as_str(), "item" | "li") {
                continue;
            }
            let prefix = if ordered {
                let value = format!("{index}. ");
                index += 1;
                value
            } else {
                format!("{marker} ")
            };
            output.extend(self.list_item(&prefix, item, width, style));
        }
        output
    }

    fn list_item(
        &mut self,
        prefix: &str,
        item: &StmlElement,
        width: usize,
        style: &StmlStyle,
    ) -> Vec<StmlLine> {
        let prefix_width = cell_width(prefix);
        let body = self.nodes(
            &item.children,
            width.saturating_sub(prefix_width).max(1),
            style,
        );
        body.into_iter()
            .enumerate()
            .map(|(index, line)| {
                let mut output = StmlLine::default();
                push_span(
                    &mut output.spans,
                    StmlStyle {
                        fg: (index == 0).then(|| "muted".into()),
                        ..StmlStyle::default()
                    }
                    .span(if index == 0 {
                        prefix.to_owned()
                    } else {
                        " ".repeat(prefix_width)
                    }),
                );
                append_line(&mut output, line);
                output
            })
            .collect()
    }

    fn code(&mut self, element: &StmlElement, width: usize, style: &StmlStyle) -> Vec<StmlLine> {
        let border_style = element
            .attrs
            .get("border-style")
            .map(String::as_str)
            .unwrap_or("single");
        let chars = BorderChars::from_name(border_style).unwrap_or_else(BorderChars::single);
        let mut code_style = style.clone();
        if let Some(foreground) = element.attrs.get("fg") {
            code_style.fg = Some(foreground.clone());
        }
        let raw = element
            .children
            .iter()
            .filter_map(|child| match child {
                StmlNode::Text(text) => Some(text.as_str()),
                StmlNode::Element(_) => None,
            })
            .collect::<String>();
        let code_width = width.saturating_sub(4).max(1);
        let content = dedent(&raw)
            .split('\n')
            .map(|line| StmlLine {
                spans: vec![code_style.span(truncate_cells(&line.replace('\t', "  "), code_width))],
            })
            .collect();
        frame_lines(
            content,
            FrameOptions {
                width,
                bordered: true,
                chars,
                border_color: element
                    .attrs
                    .get("border-color")
                    .cloned()
                    .unwrap_or_else(|| "subtle".into()),
                title: element.attrs.get("title").cloned(),
                title_color: "heading".into(),
                background: element.attrs.get("bg").cloned(),
                padding_x: 1,
                padding_y: 0,
            },
        )
    }
}

#[derive(Clone, Copy)]
struct BorderChars {
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    horizontal: char,
    vertical: char,
}

impl BorderChars {
    fn single() -> Self {
        Self {
            top_left: '┌',
            top_right: '┐',
            bottom_left: '└',
            bottom_right: '┘',
            horizontal: '─',
            vertical: '│',
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "single" => Some(Self::single()),
            "rounded" => Some(Self {
                top_left: '╭',
                top_right: '╮',
                bottom_left: '╰',
                bottom_right: '╯',
                ..Self::single()
            }),
            "double" => Some(Self {
                top_left: '╔',
                top_right: '╗',
                bottom_left: '╚',
                bottom_right: '╝',
                horizontal: '═',
                vertical: '║',
            }),
            "heavy" => Some(Self {
                top_left: '┏',
                top_right: '┓',
                bottom_left: '┗',
                bottom_right: '┛',
                horizontal: '━',
                vertical: '┃',
            }),
            _ => None,
        }
    }
}

struct FrameOptions {
    width: usize,
    bordered: bool,
    chars: BorderChars,
    border_color: String,
    title: Option<String>,
    title_color: String,
    background: Option<String>,
    padding_x: usize,
    padding_y: usize,
}

fn frame_lines(content: Vec<StmlLine>, options: FrameOptions) -> Vec<StmlLine> {
    let inner_width = options
        .width
        .saturating_sub(usize::from(options.bordered) * 2 + options.padding_x * 2)
        .max(1);
    let content = content
        .into_iter()
        .map(|line| fill_line(line, inner_width, options.background.as_deref()))
        .collect::<Vec<_>>();
    let blank = || StmlLine {
        spans: vec![
            StmlStyle {
                bg: options.background.clone(),
                ..StmlStyle::default()
            }
            .span(" ".repeat(inner_width + options.padding_x * 2)),
        ],
    };
    let mut body = Vec::new();
    body.extend((0..options.padding_y).map(|_| blank()));
    for line in content {
        let pad_style = StmlStyle {
            bg: options.background.clone(),
            ..StmlStyle::default()
        };
        let mut padded = StmlLine::default();
        push_span(
            &mut padded.spans,
            pad_style.span(" ".repeat(options.padding_x)),
        );
        append_line(&mut padded, line);
        push_span(
            &mut padded.spans,
            pad_style.span(" ".repeat(options.padding_x)),
        );
        body.push(padded);
    }
    body.extend((0..options.padding_y).map(|_| blank()));
    if !options.bordered {
        return body;
    }

    let horizontal_width = options.width.saturating_sub(2);
    let border_style = StmlStyle {
        fg: Some(options.border_color.clone()),
        ..StmlStyle::default()
    };
    let mut top = StmlLine::default();
    if let Some(title) = options
        .title
        .as_deref()
        .map(str::trim)
        .filter(|title| !title.is_empty())
    {
        let label = truncate_cells(&format!(" {title} "), horizontal_width.saturating_sub(2));
        let label_width = cell_width(&label);
        push_span(
            &mut top.spans,
            border_style.span(format!(
                "{}{}",
                options.chars.top_left, options.chars.horizontal
            )),
        );
        push_span(
            &mut top.spans,
            StmlStyle {
                fg: Some(options.title_color.clone()),
                bold: true,
                ..StmlStyle::default()
            }
            .span(label),
        );
        push_span(
            &mut top.spans,
            border_style.span(format!(
                "{}{}",
                options
                    .chars
                    .horizontal
                    .to_string()
                    .repeat(horizontal_width.saturating_sub(1 + label_width)),
                options.chars.top_right
            )),
        );
    } else {
        push_span(
            &mut top.spans,
            border_style.span(format!(
                "{}{}{}",
                options.chars.top_left,
                options
                    .chars
                    .horizontal
                    .to_string()
                    .repeat(horizontal_width),
                options.chars.top_right
            )),
        );
    }
    let mut result = vec![top];
    for line in body {
        let mut framed = StmlLine::default();
        let side_style = StmlStyle {
            fg: Some(options.border_color.clone()),
            bg: options.background.clone(),
            ..StmlStyle::default()
        };
        push_span(
            &mut framed.spans,
            side_style.span(options.chars.vertical.to_string()),
        );
        append_line(&mut framed, line);
        push_span(
            &mut framed.spans,
            side_style.span(options.chars.vertical.to_string()),
        );
        result.push(framed);
    }
    result.push(StmlLine {
        spans: vec![border_style.span(format!(
            "{}{}{}",
            options.chars.bottom_left,
            options.chars.horizontal.to_string().repeat(horizontal_width),
            options.chars.bottom_right
        ))],
    });
    result
}

fn merge_columns(columns: Vec<Vec<StmlLine>>, widths: &[usize], gap: usize) -> Vec<StmlLine> {
    let height = columns.iter().map(Vec::len).max().unwrap_or(0);
    (0..height)
        .map(|row| {
            let mut output = StmlLine::default();
            for (index, (column, width)) in columns.iter().zip(widths).enumerate() {
                if index > 0 {
                    push_span(
                        &mut output.spans,
                        StmlStyle::default().span(" ".repeat(gap)),
                    );
                }
                let line = column.get(row).cloned().unwrap_or_default();
                append_line(&mut output, fill_line(line, *width, None));
            }
            output
        })
        .collect()
}

#[derive(Clone)]
enum TokenKind {
    Word,
    Space,
    Break,
}

#[derive(Clone)]
struct Token {
    span: StmlSpan,
    kind: TokenKind,
    width: usize,
}

fn wrap_spans(spans: Vec<StmlSpan>, width: usize) -> Vec<StmlLine> {
    let tokens = tokenize(spans);
    let mut lines = Vec::new();
    let mut current = StmlLine::default();
    let mut current_width = 0usize;
    let mut started = false;
    let flush = |lines: &mut Vec<StmlLine>, current: &mut StmlLine| {
        trim_plain_end(current);
        lines.push(std::mem::take(current));
    };

    for token in tokens {
        match token.kind {
            TokenKind::Break => {
                flush(&mut lines, &mut current);
                current_width = 0;
                started = false;
            }
            TokenKind::Space => {
                if !started {
                    continue;
                }
                if current_width + token.width > width {
                    flush(&mut lines, &mut current);
                    current_width = 0;
                    started = false;
                    continue;
                }
                push_span(&mut current.spans, token.span);
                current_width += token.width;
            }
            TokenKind::Word => {
                if current_width + token.width <= width {
                    push_span(&mut current.spans, token.span);
                    current_width += token.width;
                    started = true;
                    continue;
                }
                if started {
                    flush(&mut lines, &mut current);
                    current_width = 0;
                }
                let mut rest = token.span.text.as_str();
                while cell_width(rest) > width {
                    let (slice, bytes) = truncate_cells_with_bytes(rest, width);
                    if bytes == 0 {
                        break;
                    }
                    push_span(&mut current.spans, styled(&token.span, slice));
                    flush(&mut lines, &mut current);
                    rest = &rest[bytes..];
                }
                if !rest.is_empty() {
                    push_span(&mut current.spans, styled(&token.span, rest.to_owned()));
                    current_width = cell_width(rest);
                    started = true;
                } else {
                    started = false;
                }
            }
        }
    }
    if !current.spans.is_empty() || lines.is_empty() {
        trim_plain_end(&mut current);
        lines.push(current);
    }
    lines
}

fn tokenize(spans: Vec<StmlSpan>) -> Vec<Token> {
    let mut output = Vec::new();
    for span in spans {
        let mut current = String::new();
        let mut mode = None;
        let flush =
            |output: &mut Vec<Token>, current: &mut String, mode: &mut Option<TokenKind>| {
                if let Some(kind) = mode.take() {
                    let text = std::mem::take(current);
                    let visible_space = matches!(kind, TokenKind::Space) && span.bg.is_some();
                    let kind = if visible_space { TokenKind::Word } else { kind };
                    output.push(Token {
                        width: cell_width(&text),
                        span: styled(&span, text),
                        kind,
                    });
                }
            };
        for character in span.text.chars() {
            let next = if character == '\n' {
                TokenKind::Break
            } else if character == ' ' {
                TokenKind::Space
            } else {
                TokenKind::Word
            };
            if character == '\n' {
                flush(&mut output, &mut current, &mut mode);
                output.push(Token {
                    span: styled(&span, "\n".into()),
                    kind: TokenKind::Break,
                    width: 0,
                });
            } else if mode
                .as_ref()
                .is_some_and(|mode| std::mem::discriminant(mode) != std::mem::discriminant(&next))
            {
                flush(&mut output, &mut current, &mut mode);
                mode = Some(next);
                current.push(character);
            } else {
                mode.get_or_insert(next);
                current.push(character);
            }
        }
        flush(&mut output, &mut current, &mut mode);
    }
    output
}

fn fill_line(mut line: StmlLine, width: usize, background: Option<&str>) -> StmlLine {
    if let Some(background) = background {
        for span in &mut line.spans {
            if span.bg.is_none() {
                span.bg = Some(background.to_owned());
            }
        }
    }
    let used = line_width(&line);
    if used < width {
        push_span(
            &mut line.spans,
            StmlStyle {
                bg: background.map(str::to_owned),
                ..StmlStyle::default()
            }
            .span(" ".repeat(width - used)),
        );
    }
    line
}

fn is_inline(node: &StmlNode) -> bool {
    match node {
        StmlNode::Text(_) => true,
        StmlNode::Element(element) => matches!(
            element.tag.as_str(),
            "b" | "strong"
                | "i"
                | "em"
                | "u"
                | "dim"
                | "muted"
                | "s"
                | "strike"
                | "del"
                | "c"
                | "color"
                | "span"
                | "a"
                | "link"
                | "kbd"
                | "badge"
                | "br"
        ),
    }
}

fn truthy(value: &str) -> bool {
    matches!(value, "" | "true" | "yes" | "on")
}

fn numeric_attr(element: &StmlElement, name: &str) -> Option<usize> {
    element
        .attrs
        .get(name)
        .and_then(|value| value.parse::<f64>().ok())
        .filter(|value| value.is_finite() && *value >= 0.0)
        .map(|value| value.floor() as usize)
}

fn width_attr(value: Option<&String>, available: usize) -> Option<usize> {
    let value = value?;
    if let Some(percent) = value.strip_suffix('%') {
        return percent
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .map(|value| ((available as f64 * value / 100.0).floor() as usize).max(1));
    }
    value
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| (value.floor() as usize).max(1))
}

fn collapse_whitespace(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut whitespace = false;
    for character in text.chars() {
        if character.is_whitespace() {
            if !whitespace {
                output.push(' ');
                whitespace = true;
            }
        } else {
            output.push(character);
            whitespace = false;
        }
    }
    output
}

fn dedent(text: &str) -> String {
    let text = text.strip_prefix('\n').unwrap_or(text);
    let text = text.trim_end();
    let minimum = text
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);
    text.lines()
        .map(|line| line.get(minimum..).unwrap_or(""))
        .collect::<Vec<_>>()
        .join("\n")
}

fn blank_line(line: &StmlLine) -> bool {
    line_width(line) == 0 && line.spans.iter().all(|span| span.text.trim().is_empty())
}

fn trim_plain_end(line: &mut StmlLine) {
    while let Some(last) = line.spans.last_mut() {
        if last.bg.is_some() || !last.text.chars().all(|character| character == ' ') {
            if last.bg.is_none() {
                let length = last.text.trim_end_matches(' ').len();
                last.text.truncate(length);
                if last.text.is_empty() {
                    line.spans.pop();
                    continue;
                }
            }
            break;
        }
        line.spans.pop();
    }
}

fn styled(template: &StmlSpan, text: String) -> StmlSpan {
    StmlSpan {
        text,
        ..template.clone()
    }
}

fn push_span(spans: &mut Vec<StmlSpan>, span: StmlSpan) {
    if span.text.is_empty() {
        return;
    }
    if let Some(last) = spans.last_mut()
        && last.fg == span.fg
        && last.bg == span.bg
        && last.bold == span.bold
        && last.italic == span.italic
        && last.underline == span.underline
        && last.strike == span.strike
        && last.dim == span.dim
    {
        last.text.push_str(&span.text);
    } else {
        spans.push(span);
    }
}

fn append_line(target: &mut StmlLine, source: StmlLine) {
    for span in source.spans {
        push_span(&mut target.spans, span);
    }
}

fn line_width(line: &StmlLine) -> usize {
    line.spans.iter().map(|span| cell_width(&span.text)).sum()
}

fn cell_width(text: &str) -> usize {
    UnicodeWidthStr::width(text)
}

fn truncate_cells(text: &str, width: usize) -> String {
    truncate_cells_with_bytes(text, width).0
}

fn truncate_cells_with_bytes(text: &str, width: usize) -> (String, usize) {
    let mut cells = 0;
    let mut bytes = 0;
    for character in text.chars() {
        let next = cells + character.width().unwrap_or(0);
        if next > width {
            break;
        }
        cells = next;
        bytes += character.len_utf8();
    }
    (text[..bytes].to_owned(), bytes)
}

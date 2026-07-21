use std::collections::BTreeMap;

use crate::input::sanitize_terminal_text;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StmlNode {
    Text(String),
    Element(StmlElement),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StmlElement {
    pub tag: String,
    pub attrs: BTreeMap<String, String>,
    pub children: Vec<StmlNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StmlParseResult {
    pub nodes: Vec<StmlNode>,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StmlParseLimits {
    pub max_input_bytes: usize,
    pub max_nodes: usize,
    pub max_depth: usize,
    pub max_errors: usize,
}

impl Default for StmlParseLimits {
    fn default() -> Self {
        Self {
            max_input_bytes: 64 * 1024,
            max_nodes: 2_000,
            max_depth: 32,
            max_errors: 20,
        }
    }
}

#[derive(Debug)]
enum ArenaNode {
    Text(String),
    Element {
        tag: String,
        attrs: BTreeMap<String, String>,
        children: Vec<usize>,
    },
}

pub fn parse_stml(input: &str, limits: StmlParseLimits) -> StmlParseResult {
    let (source, truncated) = truncate_utf8(input, limits.max_input_bytes);
    let mut parser = Parser {
        source,
        limits,
        arena: Vec::new(),
        roots: Vec::new(),
        stack: Vec::new(),
        errors: Vec::new(),
        node_limit_reported: false,
    };
    if truncated {
        parser.error(format!(
            "input truncated at {} byte(s)",
            limits.max_input_bytes
        ));
    }
    parser.parse();
    let nodes = parser
        .roots
        .iter()
        .map(|index| materialize(*index, &parser.arena))
        .collect();
    StmlParseResult {
        nodes,
        errors: parser.errors,
    }
}

struct Parser<'a> {
    source: &'a str,
    limits: StmlParseLimits,
    arena: Vec<ArenaNode>,
    roots: Vec<usize>,
    stack: Vec<usize>,
    errors: Vec<String>,
    node_limit_reported: bool,
}

impl Parser<'_> {
    fn parse(&mut self) {
        let mut offset = 0usize;
        while offset < self.source.len() && !self.node_limit_reported {
            let Some(relative) = self.source[offset..].find('<') else {
                self.push_text(&self.source[offset..]);
                break;
            };
            let open = offset + relative;
            if open > offset {
                self.push_text(&self.source[offset..open]);
            }
            if self.node_limit_reported {
                break;
            }
            if self.source[open..].starts_with("<!--") {
                offset = self.source[open + 4..]
                    .find("-->")
                    .map_or(self.source.len(), |end| open + 4 + end + 3);
                continue;
            }
            if self.source.as_bytes().get(open + 1) == Some(&b'/') {
                offset = self.close_tag(open);
                continue;
            }
            let Some(next) = self.source.as_bytes().get(open + 1).copied() else {
                self.push_text("<");
                break;
            };
            if !next.is_ascii_alphabetic() {
                self.push_text("<");
                offset = open + 1;
                continue;
            }
            let Some(tag) = read_open_tag(self.source, open) else {
                self.push_text("<");
                offset = open + 1;
                continue;
            };
            offset = tag.next;
            if self.stack.len() >= self.limits.max_depth {
                self.error(format!(
                    "depth limit reached at <{}> ({} level(s))",
                    tag.tag, self.limits.max_depth
                ));
                continue;
            }
            let Some(index) = self.push_element(tag.tag.clone(), tag.attrs) else {
                break;
            };
            if tag.self_closing || is_void(&tag.tag) {
                continue;
            }
            if is_raw(&tag.tag) {
                let closer = format!("</{}", tag.tag);
                if let Some(end) = find_case_insensitive(self.source, offset, &closer) {
                    self.push_text_child(index, &self.source[offset..end]);
                    offset = self.source[end..]
                        .find('>')
                        .map_or(self.source.len(), |close| end + close + 1);
                } else {
                    self.push_text_child(index, &self.source[offset..]);
                    self.error(format!("unclosed <{}>", tag.tag));
                    offset = self.source.len();
                }
                continue;
            }
            self.stack.push(index);
        }
        if !self.stack.is_empty() {
            let tags = self
                .stack
                .iter()
                .filter_map(|index| match &self.arena[*index] {
                    ArenaNode::Element { tag, .. } => Some(format!("<{tag}>")),
                    ArenaNode::Text(_) => None,
                })
                .collect::<Vec<_>>()
                .join(", ");
            self.error(format!("unclosed tag(s): {tags}"));
        }
    }

    fn close_tag(&mut self, open: usize) -> usize {
        let mut cursor = open + 2;
        let start = cursor;
        while self
            .source
            .as_bytes()
            .get(cursor)
            .is_some_and(|byte| is_name_byte(*byte))
        {
            cursor += 1;
        }
        let name = self.source[start..cursor].to_ascii_lowercase();
        let next = self.source[cursor..]
            .find('>')
            .map_or(self.source.len(), |end| cursor + end + 1);
        let matching = self.stack.iter().rposition(
            |index| matches!(&self.arena[*index], ArenaNode::Element { tag, .. } if tag == &name),
        );
        match matching {
            None => self.error(format!("stray closing tag </{name}>")),
            Some(index) => {
                let implicitly_closed = self.stack.len().saturating_sub(index + 1);
                if implicitly_closed > 0 {
                    self.error(format!(
                        "closing </{name}> implicitly closed {implicitly_closed} tag(s)"
                    ));
                }
                self.stack.truncate(index);
            }
        }
        next
    }

    fn push_element(&mut self, tag: String, attrs: BTreeMap<String, String>) -> Option<usize> {
        if !self.reserve_node() {
            return None;
        }
        let index = self.arena.len();
        self.arena.push(ArenaNode::Element {
            tag,
            attrs,
            children: Vec::new(),
        });
        self.attach(index);
        Some(index)
    }

    fn push_text(&mut self, text: &str) {
        let text = sanitize(text);
        if text.is_empty() {
            return;
        }
        let siblings = self.current_children();
        if let Some(last) = siblings.last().copied()
            && let ArenaNode::Text(existing) = &mut self.arena[last]
        {
            existing.push_str(&text);
            return;
        }
        if !self.reserve_node() {
            return;
        }
        let index = self.arena.len();
        self.arena.push(ArenaNode::Text(text));
        self.attach(index);
    }

    fn push_text_child(&mut self, parent: usize, text: &str) {
        let text = sanitize(text);
        if text.is_empty() || !self.reserve_node() {
            return;
        }
        let index = self.arena.len();
        self.arena.push(ArenaNode::Text(text));
        if let ArenaNode::Element { children, .. } = &mut self.arena[parent] {
            children.push(index);
        }
    }

    fn current_children(&self) -> &[usize] {
        self.stack
            .last()
            .map_or(&self.roots, |parent| match &self.arena[*parent] {
                ArenaNode::Element { children, .. } => children,
                ArenaNode::Text(_) => unreachable!("text nodes cannot be parents"),
            })
    }

    fn attach(&mut self, index: usize) {
        if let Some(parent) = self.stack.last().copied() {
            if let ArenaNode::Element { children, .. } = &mut self.arena[parent] {
                children.push(index);
            }
        } else {
            self.roots.push(index);
        }
    }

    fn reserve_node(&mut self) -> bool {
        if self.arena.len() < self.limits.max_nodes {
            return true;
        }
        if !self.node_limit_reported {
            self.error(format!(
                "node limit reached at {} node(s); remaining markup ignored",
                self.limits.max_nodes
            ));
            self.node_limit_reported = true;
        }
        false
    }

    fn error(&mut self, message: String) {
        if self.errors.len() < self.limits.max_errors {
            self.errors.push(message);
        }
    }
}

struct OpenTag {
    tag: String,
    attrs: BTreeMap<String, String>,
    self_closing: bool,
    next: usize,
}

fn read_open_tag(source: &str, open: usize) -> Option<OpenTag> {
    let bytes = source.as_bytes();
    let mut cursor = open + 1;
    let start = cursor;
    while bytes.get(cursor).is_some_and(|byte| is_name_byte(*byte)) {
        cursor += 1;
    }
    if cursor == start {
        return None;
    }
    let tag = source[start..cursor].to_ascii_lowercase();
    let mut attrs = BTreeMap::new();
    let mut self_closing = false;
    loop {
        skip_space(bytes, &mut cursor);
        match bytes.get(cursor).copied()? {
            b'>' => {
                cursor += 1;
                break;
            }
            b'/' if bytes.get(cursor + 1) == Some(&b'>') => {
                cursor += 2;
                self_closing = true;
                break;
            }
            _ => {}
        }
        let name_start = cursor;
        while bytes.get(cursor).is_some_and(|byte| is_name_byte(*byte)) {
            cursor += 1;
        }
        if cursor == name_start {
            cursor += 1;
            continue;
        }
        let name = source[name_start..cursor].to_ascii_lowercase();
        skip_space(bytes, &mut cursor);
        let value = if bytes.get(cursor) == Some(&b'=') {
            cursor += 1;
            skip_space(bytes, &mut cursor);
            read_attribute_value(source, &mut cursor)
        } else {
            String::new()
        };
        attrs.insert(name, sanitize(&value));
    }
    Some(OpenTag {
        tag,
        attrs,
        self_closing,
        next: cursor,
    })
}

fn read_attribute_value(source: &str, cursor: &mut usize) -> String {
    let bytes = source.as_bytes();
    if matches!(bytes.get(*cursor), Some(b'"' | b'\'')) {
        let quote = bytes[*cursor];
        *cursor += 1;
        let start = *cursor;
        while bytes.get(*cursor).is_some_and(|byte| *byte != quote) {
            *cursor += 1;
        }
        let value = source[start..*cursor].to_owned();
        if bytes.get(*cursor) == Some(&quote) {
            *cursor += 1;
        }
        value
    } else {
        let start = *cursor;
        while bytes
            .get(*cursor)
            .is_some_and(|byte| !byte.is_ascii_whitespace() && !matches!(*byte, b'>' | b'/'))
        {
            *cursor += 1;
        }
        source[start..*cursor].to_owned()
    }
}

fn materialize(index: usize, arena: &[ArenaNode]) -> StmlNode {
    match &arena[index] {
        ArenaNode::Text(text) => StmlNode::Text(text.clone()),
        ArenaNode::Element {
            tag,
            attrs,
            children,
        } => StmlNode::Element(StmlElement {
            tag: tag.clone(),
            attrs: attrs.clone(),
            children: children
                .iter()
                .map(|child| materialize(*child, arena))
                .collect(),
        }),
    }
}

pub fn decode_stml_entities(text: &str) -> String {
    let mut output = String::with_capacity(text.len());
    let mut offset = 0usize;
    while let Some(relative) = text[offset..].find('&') {
        let start = offset + relative;
        output.push_str(&text[offset..start]);
        let Some(end_relative) = text[start + 1..].find(';') else {
            output.push_str(&text[start..]);
            return output;
        };
        let end = start + 1 + end_relative;
        let body = &text[start + 1..end];
        if let Some(decoded) = decode_entity(body) {
            output.push(decoded);
        } else {
            output.push_str(&text[start..=end]);
        }
        offset = end + 1;
    }
    output.push_str(&text[offset..]);
    output
}

fn decode_entity(body: &str) -> Option<char> {
    if let Some(numeric) = body.strip_prefix("#x").or_else(|| body.strip_prefix("#X")) {
        return u32::from_str_radix(numeric, 16)
            .ok()
            .and_then(char::from_u32);
    }
    if let Some(numeric) = body.strip_prefix('#') {
        return numeric.parse::<u32>().ok().and_then(char::from_u32);
    }
    match body.to_ascii_lowercase().as_str() {
        "amp" => Some('&'),
        "lt" => Some('<'),
        "gt" => Some('>'),
        "quot" => Some('"'),
        "apos" => Some('\''),
        "nbsp" => Some(' '),
        "mdash" => Some('—'),
        "ndash" => Some('–'),
        "hellip" => Some('…'),
        "bull" => Some('•'),
        "middot" => Some('·'),
        "rarr" => Some('→'),
        "larr" => Some('←'),
        "uarr" => Some('↑'),
        "darr" => Some('↓'),
        "check" => Some('✓'),
        "cross" => Some('✗'),
        "times" => Some('×'),
        _ => None,
    }
}

fn truncate_utf8(input: &str, max_bytes: usize) -> (&str, bool) {
    if input.len() <= max_bytes {
        return (input, false);
    }
    let mut end = max_bytes.min(input.len());
    while end > 0 && !input.is_char_boundary(end) {
        end -= 1;
    }
    (&input[..end], true)
}

fn sanitize(text: &str) -> String {
    sanitize_terminal_text(text, false).replace('\t', "")
}

fn is_void(tag: &str) -> bool {
    matches!(tag, "br" | "hr" | "rule" | "divider" | "spacer" | "space")
}

fn is_raw(tag: &str) -> bool {
    matches!(tag, "code" | "pre")
}

fn is_name_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
}

fn skip_space(bytes: &[u8], cursor: &mut usize) {
    while bytes
        .get(*cursor)
        .is_some_and(|byte| byte.is_ascii_whitespace())
    {
        *cursor += 1;
    }
}

fn find_case_insensitive(source: &str, from: usize, needle: &str) -> Option<usize> {
    source[from..]
        .to_ascii_lowercase()
        .find(&needle.to_ascii_lowercase())
        .map(|relative| from + relative)
}

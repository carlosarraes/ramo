mod command;
mod layout;
mod parse;
mod render;

pub use command::{MarkupColor, MarkupRenderOptions, guide, render};
pub use layout::{
    MIN_STML_LAYOUT_WIDTH, STML_REFERENCE_WIDTH, StmlLayoutResult, StmlLine, StmlSpan, StmlStyle,
    layout_stml, layout_stml_cached,
};
pub use parse::{
    StmlElement, StmlNode, StmlParseLimits, StmlParseResult, decode_stml_entities, parse_stml,
};
pub use render::{
    StmlTextRenderResult, render_stml_to_ansi, render_stml_to_text, resolve_stml_color,
};

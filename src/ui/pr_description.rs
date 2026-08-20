use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::remote_review::PullRequestReviewContext;

use super::document::{ScrollableDocument, render_document};
use super::themes::AppTheme;

pub const EMPTY_DESCRIPTION: &str = "This pull request has no description.";
const FOOTER_HELP: &str = "j/k scroll · d/u half page · M/L/P/C switch · Ctrl-Q back";

pub fn new_document(body: &str, width: u16) -> ScrollableDocument {
    ScrollableDocument::new(body, EMPTY_DESCRIPTION, width)
}

pub struct PrDescriptionWidget<'a> {
    context: &'a PullRequestReviewContext,
    document: &'a mut ScrollableDocument,
    theme: &'a AppTheme,
    notice: Option<&'a str>,
}

impl<'a> PrDescriptionWidget<'a> {
    pub fn new(
        context: &'a PullRequestReviewContext,
        document: &'a mut ScrollableDocument,
        theme: &'a AppTheme,
    ) -> Self {
        Self {
            context,
            document,
            theme,
            notice: None,
        }
    }

    /// Replaces the help footer with a message. Full-screen overlays cover the review footer, so
    /// without this a refusal like "Linear tickets are off" would be reported to nobody.
    pub fn notice(mut self, notice: Option<&'a str>) -> Self {
        self.notice = notice;
        self
    }
}

impl Widget for PrDescriptionWidget<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        self.document.fit(
            &self.context.body,
            EMPTY_DESCRIPTION,
            area.width,
            area.height,
        );
        render_document(
            area,
            buffer,
            self.theme,
            &format!("PR #{} · {}", self.context.number, self.context.title),
            &format!(
                "{} wants to merge {} into {}",
                self.context.author_login, self.context.head_ref, self.context.base_ref
            ),
            self.notice.unwrap_or(FOOTER_HELP),
            self.document,
        );
    }
}

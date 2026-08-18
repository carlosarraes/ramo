use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::widgets::Widget;

use crate::linear::LinearTicket;

use super::document::{ScrollableDocument, render_document};
use super::themes::AppTheme;

pub const EMPTY_DESCRIPTION: &str = "This ticket has no description.";
const FOOTER_HELP: &str = "j/k scroll · d/u half page · g/G ends · L back";

pub fn new_document(description: &str, width: u16) -> ScrollableDocument {
    ScrollableDocument::new(description, EMPTY_DESCRIPTION, width)
}

pub struct LinearTicketWidget<'a> {
    ticket: &'a LinearTicket,
    document: &'a mut ScrollableDocument,
    theme: &'a AppTheme,
    /// Set when Linear's own GitHub link names a different PR than the one under review.
    mismatch: Option<u64>,
}

impl<'a> LinearTicketWidget<'a> {
    pub fn new(
        ticket: &'a LinearTicket,
        document: &'a mut ScrollableDocument,
        theme: &'a AppTheme,
        mismatch: Option<u64>,
    ) -> Self {
        Self {
            ticket,
            document,
            theme,
            mismatch,
        }
    }
}

impl Widget for LinearTicketWidget<'_> {
    fn render(self, area: Rect, buffer: &mut Buffer) {
        self.document.fit(
            &self.ticket.description,
            EMPTY_DESCRIPTION,
            area.width,
            area.height,
        );
        let subtitle = match self.mismatch {
            Some(number) => format!(
                "⚠ Linear links this ticket to PR #{number}, not the one open here · {}",
                self.ticket.subtitle()
            ),
            None => self.ticket.subtitle(),
        };
        render_document(
            area,
            buffer,
            self.theme,
            &format!("{} · {}", self.ticket.identifier, self.ticket.title),
            &subtitle,
            FOOTER_HELP,
            self.document,
        );
    }
}

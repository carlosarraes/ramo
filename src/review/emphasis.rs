use similar::{ChangeTag, TextDiff};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangedSpan {
    pub text: String,
    pub emphasized: bool,
}

impl ChangedSpan {
    pub(crate) fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            emphasized: false,
        }
    }

    pub(crate) fn emphasized(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            emphasized: true,
        }
    }
}

pub(crate) fn emphasize_pair(old: &str, new: &str) -> (Vec<ChangedSpan>, Vec<ChangedSpan>) {
    let mut old_spans = Vec::new();
    let mut new_spans = Vec::new();
    for change in TextDiff::from_chars(old, new).iter_all_changes() {
        match change.tag() {
            ChangeTag::Equal => {
                push_span(&mut old_spans, change.value(), false);
                push_span(&mut new_spans, change.value(), false);
            }
            ChangeTag::Delete => push_span(&mut old_spans, change.value(), true),
            ChangeTag::Insert => push_span(&mut new_spans, change.value(), true),
        }
    }
    (old_spans, new_spans)
}

fn push_span(spans: &mut Vec<ChangedSpan>, text: &str, emphasized: bool) {
    if text.is_empty() {
        return;
    }
    if let Some(previous) = spans.last_mut()
        && previous.emphasized == emphasized
    {
        previous.text.push_str(text);
        return;
    }
    spans.push(ChangedSpan {
        text: text.into(),
        emphasized,
    });
}

#[cfg(test)]
mod tests {
    use super::{ChangedSpan, emphasize_pair};

    #[test]
    fn character_emphasis_keeps_common_text_neutral() {
        let (old, new) = emphasize_pair("let value = old();", "let value = new();");

        assert_eq!(
            old,
            vec![
                ChangedSpan::plain("let value = "),
                ChangedSpan::emphasized("old"),
                ChangedSpan::plain("();"),
            ]
        );
        assert_eq!(
            new,
            vec![
                ChangedSpan::plain("let value = "),
                ChangedSpan::emphasized("new"),
                ChangedSpan::plain("();"),
            ]
        );
    }

    #[test]
    fn emphasis_handles_insertions_and_empty_lines() {
        let (old, new) = emphasize_pair("", "added");
        assert!(old.is_empty());
        assert_eq!(new, vec![ChangedSpan::emphasized("added")]);
    }
}

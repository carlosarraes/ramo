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

/// A pair that changes more than this fraction of its characters across several
/// separate runs is treated as a rewrite: the line keeps a flat background instead
/// of alternating between emphasized and plain fragments. A single contiguous change
/// is always shown, however large, because that is the edit the reader is looking for.
const MAX_CHANGED_RATIO: f64 = 0.6;

pub(crate) fn emphasize_pair(old: &str, new: &str) -> (Vec<ChangedSpan>, Vec<ChangedSpan>) {
    let mut old_spans = Vec::new();
    let mut new_spans = Vec::new();
    for change in TextDiff::from_unicode_words(old, new).iter_all_changes() {
        let value = change.value();
        // Whitespace-only runs stay flat so re-indentation never paints a dark block.
        let emphasized = !value.trim().is_empty();
        match change.tag() {
            ChangeTag::Equal => {
                push_span(&mut old_spans, value, false);
                push_span(&mut new_spans, value, false);
            }
            ChangeTag::Delete => push_span(&mut old_spans, value, emphasized),
            ChangeTag::Insert => push_span(&mut new_spans, value, emphasized),
        }
    }
    let fragmented = changed_runs(&old_spans).max(changed_runs(&new_spans)) > 1;
    let rewritten = fragmented
        && !old.trim().is_empty()
        && !new.trim().is_empty()
        && (changed_ratio(&old_spans) > MAX_CHANGED_RATIO
            || changed_ratio(&new_spans) > MAX_CHANGED_RATIO);
    if rewritten {
        return (flatten(old_spans), flatten(new_spans));
    }
    (old_spans, new_spans)
}

fn changed_runs(spans: &[ChangedSpan]) -> usize {
    spans.iter().filter(|span| span.emphasized).count()
}

fn changed_ratio(spans: &[ChangedSpan]) -> f64 {
    let mut total = 0usize;
    let mut changed = 0usize;
    for span in spans {
        let width = span.text.chars().filter(|c| !c.is_whitespace()).count();
        total += width;
        if span.emphasized {
            changed += width;
        }
    }
    if total == 0 {
        return 0.0;
    }
    changed as f64 / total as f64
}

fn flatten(spans: Vec<ChangedSpan>) -> Vec<ChangedSpan> {
    let mut flattened = Vec::new();
    for span in spans {
        push_span(&mut flattened, &span.text, false);
    }
    flattened
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
    fn word_emphasis_keeps_common_text_neutral() {
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

    #[test]
    fn indentation_only_changes_stay_flat() {
        let (old, new) = emphasize_pair("  value = 1", "      value = 1");

        for spans in [&old, &new] {
            assert!(
                spans.iter().all(|span| !span.emphasized),
                "indentation should not be emphasized: {spans:?}"
            );
        }
    }

    #[test]
    fn rewritten_pairs_drop_emphasis_instead_of_speckling() {
        let (old, new) = emphasize_pair(
            "            logger.error(",
            "        hosts = \", \".join(_uri_hosts(uri)) or \"unparseable host\"",
        );

        for spans in [&old, &new] {
            assert!(
                spans.iter().all(|span| !span.emphasized),
                "a rewritten pair should render flat: {spans:?}"
            );
        }
    }

    #[test]
    fn a_single_contiguous_change_survives_the_rewrite_gate() {
        let (old, new) = emphasize_pair("  old_one();", "  new_one();");

        assert_eq!(
            old.iter()
                .filter(|span| span.emphasized)
                .map(|span| span.text.clone())
                .collect::<Vec<_>>(),
            vec!["old_one".to_owned()]
        );
        assert_eq!(
            new.iter()
                .filter(|span| span.emphasized)
                .map(|span| span.text.clone())
                .collect::<Vec<_>>(),
            vec!["new_one".to_owned()]
        );
    }

    #[test]
    fn renaming_one_token_emphasizes_only_that_token() {
        let (old, new) = emphasize_pair(
            "    sys.exit(_abort(message))",
            "    sys.exit(_halt(message))",
        );

        let emphasized = |spans: &[ChangedSpan]| {
            spans
                .iter()
                .filter(|span| span.emphasized)
                .map(|span| span.text.clone())
                .collect::<Vec<_>>()
        };
        assert_eq!(emphasized(&old), vec!["_abort".to_owned()]);
        assert_eq!(emphasized(&new), vec!["_halt".to_owned()]);
    }
}

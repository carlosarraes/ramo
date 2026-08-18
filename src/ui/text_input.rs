/// An edit buffer with a caret, shared by every text-entry surface.
///
/// The caret is a **char** index rather than a byte index, so every motion is safe across
/// multi-byte characters. Callers keep handing whole strings to the review controllers, which
/// is why this type owns no rendering and no wrapping — only the text and where the caret sits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextInput {
    value: String,
    caret: usize,
}

impl TextInput {
    pub fn new() -> Self {
        Self::default()
    }

    /// Seeds an existing value with the caret at the end, which is where a reviewer expects it
    /// when reopening a saved note or the generated overall comment.
    pub fn with_value(value: impl Into<String>) -> Self {
        let value = value.into();
        let caret = value.chars().count();
        Self { value, caret }
    }

    pub fn value(&self) -> &str {
        &self.value
    }

    pub fn caret(&self) -> usize {
        self.caret
    }

    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    pub fn char_count(&self) -> usize {
        self.value.chars().count()
    }

    pub fn clear(&mut self) {
        self.value.clear();
        self.caret = 0;
    }

    /// Replaces the text wholesale, clamping the caret. Used when an external source (a saved
    /// note being edited) supplies the buffer.
    pub fn set_value(&mut self, value: impl Into<String>) {
        self.value = value.into();
        self.caret = self.caret.min(self.char_count());
    }

    pub fn insert(&mut self, character: char) {
        let at = self.byte_index(self.caret);
        self.value.insert(at, character);
        self.caret += 1;
    }

    pub fn backspace(&mut self) -> bool {
        if self.caret == 0 {
            return false;
        }
        let end = self.byte_index(self.caret);
        let start = self.byte_index(self.caret - 1);
        self.value.replace_range(start..end, "");
        self.caret -= 1;
        true
    }

    pub fn delete_forward(&mut self) -> bool {
        if self.caret >= self.char_count() {
            return false;
        }
        let start = self.byte_index(self.caret);
        let end = self.byte_index(self.caret + 1);
        self.value.replace_range(start..end, "");
        true
    }

    pub fn move_home(&mut self) {
        self.caret = 0;
    }

    pub fn move_end(&mut self) {
        self.caret = self.char_count();
    }

    pub fn move_left(&mut self) {
        self.caret = self.caret.saturating_sub(1);
    }

    pub fn move_right(&mut self) {
        self.caret = (self.caret + 1).min(self.char_count());
    }

    /// Readline word motion: skip any separators, then the word itself.
    pub fn move_word_left(&mut self) {
        let chars = self.chars();
        let mut at = self.caret;
        while at > 0 && !is_word(chars[at - 1]) {
            at -= 1;
        }
        while at > 0 && is_word(chars[at - 1]) {
            at -= 1;
        }
        self.caret = at;
    }

    pub fn move_word_right(&mut self) {
        let chars = self.chars();
        let mut at = self.caret;
        while at < chars.len() && !is_word(chars[at]) {
            at += 1;
        }
        while at < chars.len() && is_word(chars[at]) {
            at += 1;
        }
        self.caret = at;
    }

    /// `Ctrl-U`. Bash kills from the caret to the start of the line, which only looks like
    /// "clear the line" because the caret is usually already at the end.
    pub fn kill_to_start(&mut self) -> bool {
        if self.caret == 0 {
            return false;
        }
        let end = self.byte_index(self.caret);
        self.value.replace_range(..end, "");
        self.caret = 0;
        true
    }

    /// `Ctrl-K`.
    pub fn kill_to_end(&mut self) -> bool {
        if self.caret >= self.char_count() {
            return false;
        }
        let start = self.byte_index(self.caret);
        self.value.truncate(start);
        true
    }

    /// `Ctrl-W`. Bash deletes back over whitespace and then over one whitespace-delimited run,
    /// which is deliberately coarser than the alphanumeric word used by `Alt-B`.
    pub fn delete_word_back(&mut self) -> bool {
        let chars = self.chars();
        let mut at = self.caret;
        while at > 0 && chars[at - 1].is_whitespace() {
            at -= 1;
        }
        while at > 0 && !chars[at - 1].is_whitespace() {
            at -= 1;
        }
        if at == self.caret {
            return false;
        }
        let end = self.byte_index(self.caret);
        let start = self.byte_index(at);
        self.value.replace_range(start..end, "");
        self.caret = at;
        true
    }

    fn chars(&self) -> Vec<char> {
        self.value.chars().collect()
    }

    fn byte_index(&self, caret: usize) -> usize {
        self.value
            .char_indices()
            .nth(caret)
            .map(|(index, _)| index)
            .unwrap_or(self.value.len())
    }
}

/// Matches the word definition already used for double-click selection in
/// `crate::review::selection`, so word motion feels the same everywhere in the app.
fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(value: &str, caret: usize) -> TextInput {
        let mut input = TextInput::with_value(value);
        input.caret = caret;
        input
    }

    #[test]
    fn a_seeded_value_puts_the_caret_at_the_end() {
        let input = TextInput::with_value("hello");
        assert_eq!(input.caret(), 5);
        assert_eq!(input.value(), "hello");
    }

    #[test]
    fn insert_and_delete_happen_at_the_caret_not_the_end() {
        let mut input = at("helo", 3);
        input.insert('l');
        assert_eq!(input.value(), "hello");
        assert_eq!(input.caret(), 4);

        assert!(input.backspace());
        assert_eq!(input.value(), "helo");
        assert_eq!(input.caret(), 3);

        assert!(input.delete_forward());
        assert_eq!(input.value(), "hel");
        assert_eq!(input.caret(), 3, "delete-forward leaves the caret put");
    }

    #[test]
    fn motions_clamp_at_both_ends() {
        let mut input = at("abc", 0);
        input.move_left();
        assert_eq!(input.caret(), 0);
        assert!(!input.backspace(), "nothing to delete at the start");

        input.move_end();
        assert_eq!(input.caret(), 3);
        input.move_right();
        assert_eq!(input.caret(), 3);
        assert!(!input.delete_forward(), "nothing to delete at the end");

        input.move_home();
        assert_eq!(input.caret(), 0);
    }

    #[test]
    fn ctrl_u_kills_to_the_start_and_ctrl_k_to_the_end() {
        let mut input = at("hello world", 6);
        assert!(input.kill_to_start());
        assert_eq!(input.value(), "world");
        assert_eq!(input.caret(), 0);

        let mut input = at("hello world", 5);
        assert!(input.kill_to_end());
        assert_eq!(input.value(), "hello");
        assert_eq!(input.caret(), 5);

        // At the end of the line Ctrl-U clears everything, which is the common case.
        let mut input = TextInput::with_value("hello world");
        assert!(input.kill_to_start());
        assert!(input.is_empty());
    }

    #[test]
    fn ctrl_w_deletes_one_whitespace_delimited_run_including_punctuation() {
        let mut input = TextInput::with_value("fix the retry_loop()");
        assert!(input.delete_word_back());
        assert_eq!(
            input.value(),
            "fix the ",
            "bash's Ctrl-W is whitespace-delimited, so punctuation goes with the word"
        );

        // Trailing whitespace is consumed before the word itself.
        let mut input = TextInput::with_value("one two   ");
        assert!(input.delete_word_back());
        assert_eq!(input.value(), "one ");

        let mut input = at("abc", 0);
        assert!(
            !input.delete_word_back(),
            "no-op at the start reports false"
        );
    }

    #[test]
    fn alt_word_motion_uses_alphanumeric_words() {
        let mut input = TextInput::with_value("retry_loop(attempt)");
        input.move_word_left();
        assert_eq!(input.caret(), 11, "lands before `attempt`");
        input.move_word_left();
        assert_eq!(
            input.caret(),
            0,
            "`retry_loop` is one word because _ is a word char"
        );

        input.move_word_right();
        assert_eq!(input.caret(), 10, "ends after `retry_loop`");
    }

    #[test]
    fn every_operation_is_char_indexed_not_byte_indexed() {
        // Two multi-byte characters; a byte-indexed implementation would panic or corrupt.
        let mut input = TextInput::with_value("héllo wörld");
        input.move_home();
        input.move_right();
        input.insert('X');
        assert_eq!(input.value(), "hXéllo wörld");

        let mut input = at("héllo", 2);
        assert!(input.backspace());
        assert_eq!(input.value(), "hllo", "deletes the accented char whole");

        let mut input = TextInput::with_value("日本語");
        assert_eq!(input.caret(), 3);
        assert!(input.backspace());
        assert_eq!(input.value(), "日本");
    }

    #[test]
    fn set_value_clamps_a_caret_past_the_new_end() {
        let mut input = TextInput::with_value("a long value");
        input.set_value("short");
        assert_eq!(input.caret(), 5);
        assert_eq!(input.value(), "short");
    }
}

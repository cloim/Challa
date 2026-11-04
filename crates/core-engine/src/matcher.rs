use crate::snippet::{Snippet, TriggerMode};
use std::collections::HashMap;

/// Result of a successful snippet keyword match.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchResult {
    /// The matched snippet definition.
    pub snippet: Snippet,
    /// Number of characters that must be erased (keyword + delimiter).
    pub backspace_count: usize,
}

/// Suffix Trie node for reverse keyword lookup.
#[derive(Debug, Default)]
struct TrieNode {
    children: HashMap<char, TrieNode>,
    snippets: Vec<Snippet>,
}

/// High-speed snippet matcher using a reverse prefix Trie.
#[derive(Debug, Default)]
pub struct SnippetMatcher {
    root: TrieNode,
    snippets_by_id: HashMap<String, Snippet>,
}

impl SnippetMatcher {
    /// Creates a new empty `SnippetMatcher`.
    pub fn new() -> Self {
        Self {
            root: TrieNode::default(),
            snippets_by_id: HashMap::new(),
        }
    }

    /// Adds or replaces a snippet in the matcher.
    pub fn add_snippet(&mut self, snippet: Snippet) {
        if !snippet.enabled {
            self.remove_snippet(&snippet.id);
            return;
        }

        // Remove old entry with same id if exists
        self.remove_snippet(&snippet.id);

        self.snippets_by_id.insert(snippet.id.clone(), snippet.clone());
        self.rebuild_trie();
    }

    /// Removes a snippet by ID.
    pub fn remove_snippet(&mut self, snippet_id: &str) -> bool {
        if self.snippets_by_id.remove(snippet_id).is_some() {
            self.rebuild_trie();
            true
        } else {
            false
        }
    }

    /// Rebuilds the reverse Trie from the registered snippets.
    fn rebuild_trie(&mut self) {
        self.root = TrieNode::default();

        for snippet in self.snippets_by_id.values() {
            if !snippet.enabled || snippet.keyword.is_empty() {
                continue;
            }

            // Insert reversed keyword
            let mut curr = &mut self.root;
            for ch in snippet.keyword.chars().rev() {
                curr = curr.children.entry(ch).or_default();
            }
            curr.snippets.push(snippet.clone());
        }
    }

    /// Returns all registered snippets.
    pub fn list_snippets(&self) -> Vec<&Snippet> {
        self.snippets_by_id.values().collect()
    }

    /// Checks if a character is a standard delimiter for Delimiter trigger mode.
    fn is_standard_delimiter(ch: char) -> bool {
        matches!(ch, ' ' | '\t' | '\r' | '\n' | ',' | '.' | ';' | ':' | '!' | '?' | '/' | '-')
    }

    /// Searches the suffix of the given buffer string for matching snippet keywords.
    /// Returns the best matching snippet and the required backspace count.
    pub fn find_match(&self, buffer: &str) -> Option<MatchResult> {
        if buffer.is_empty() || self.snippets_by_id.is_empty() {
            return None;
        }

        let chars: Vec<char> = buffer.chars().collect();
        let total_len = chars.len();

        let mut best_match: Option<MatchResult> = None;

        // Case 1: Check Immediate trigger (buffer suffix matches keyword directly)
        if let Some(m) = self.match_suffix_at(&chars, total_len, TriggerMode::Immediate, 0) {
            best_match = Some(m);
        }

        // Case 2: Check Trailing Triggers (Space, Enter, Tab, Delimiter)
        if total_len >= 2 {
            let last_char = chars[total_len - 1];

            // Space trigger
            if last_char == ' ' {
                if let Some(m) = self.match_suffix_at(&chars, total_len - 1, TriggerMode::Space, 1) {
                    if best_match.as_ref().map_or(true, |b| m.backspace_count > b.backspace_count) {
                        best_match = Some(m);
                    }
                }
            }

            // Enter trigger
            if last_char == '\n' || last_char == '\r' {
                if let Some(m) = self.match_suffix_at(&chars, total_len - 1, TriggerMode::Enter, 1) {
                    if best_match.as_ref().map_or(true, |b| m.backspace_count > b.backspace_count) {
                        best_match = Some(m);
                    }
                }
            }

            // Tab trigger
            if last_char == '\t' {
                if let Some(m) = self.match_suffix_at(&chars, total_len - 1, TriggerMode::Tab, 1) {
                    if best_match.as_ref().map_or(true, |b| m.backspace_count > b.backspace_count) {
                        best_match = Some(m);
                    }
                }
            }

            // Delimiter trigger
            if Self::is_standard_delimiter(last_char) {
                if let Some(m) = self.match_suffix_at(&chars, total_len - 1, TriggerMode::Delimiter, 1) {
                    if best_match.as_ref().map_or(true, |b| m.backspace_count > b.backspace_count) {
                        best_match = Some(m);
                    }
                }
            }
        }

        best_match
    }

    /// Walks backwards from `end_idx` in `chars` against the reverse Trie.
    fn match_suffix_at(
        &self,
        chars: &[char],
        end_idx: usize,
        expected_mode: TriggerMode,
        extra_backspaces: usize,
    ) -> Option<MatchResult> {
        let mut curr = &self.root;
        let mut matched_snippet: Option<(Snippet, usize)> = None;

        for offset in 0..end_idx {
            let ch = chars[end_idx - 1 - offset];
            if let Some(next_node) = curr.children.get(&ch) {
                curr = next_node;
                // Check if any snippet terminating here matches the required mode
                for s in &curr.snippets {
                    if s.trigger_mode == expected_mode {
                        let kw_len = s.keyword.chars().count();
                        matched_snippet = Some((s.clone(), kw_len + extra_backspaces));
                    }
                }
            } else {
                break;
            }
        }

        matched_snippet.map(|(snippet, backspace_count)| MatchResult {
            snippet,
            backspace_count,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snippet::SnippetContent;

    #[test]
    fn test_immediate_match() {
        let mut matcher = SnippetMatcher::new();
        matcher.add_snippet(Snippet::plain_text("1", "Email", ":mail", "user@example.com"));

        assert_eq!(matcher.find_match("hello :mai"), None);

        let match_res = matcher.find_match("hello :mail").expect("Should match");
        assert_eq!(match_res.snippet.id, "1");
        assert_eq!(match_res.backspace_count, 5); // ":mail".len() == 5
        assert_eq!(
            match_res.snippet.content,
            SnippetContent::PlainText("user@example.com".into())
        );
    }

    #[test]
    fn test_space_trigger_match() {
        let mut matcher = SnippetMatcher::new();
        matcher.add_snippet(Snippet::plain_text_with_trigger(
            "2",
            "Signature",
            "br",
            "Best regards,\nChalla",
            TriggerMode::Space,
        ));

        // Immediate typing 'br' should NOT trigger yet
        assert_eq!(matcher.find_match("testing br"), None);

        // Typing space after 'br' triggers!
        let match_res = matcher.find_match("testing br ").expect("Should match on space");
        assert_eq!(match_res.snippet.id, "2");
        assert_eq!(match_res.backspace_count, 3); // "br".len() + 1 (' ')
    }

    #[test]
    fn test_delimiter_trigger_match() {
        let mut matcher = SnippetMatcher::new();
        matcher.add_snippet(Snippet::plain_text_with_trigger(
            "3",
            "Short",
            "omw",
            "on my way",
            TriggerMode::Delimiter,
        ));

        assert_eq!(matcher.find_match("hey omw"), None);
        let match_dot = matcher.find_match("hey omw.").expect("Should match on period");
        assert_eq!(match_dot.backspace_count, 4); // "omw" (3) + "." (1)

        let match_comma = matcher.find_match("hey omw,").expect("Should match on comma");
        assert_eq!(match_comma.backspace_count, 4);
    }

    #[test]
    fn test_longest_match_priority() {
        let mut matcher = SnippetMatcher::new();
        matcher.add_snippet(Snippet::plain_text("1", "Short", ":d", "short"));
        matcher.add_snippet(Snippet::plain_text("2", "Long", ":date", "2026-11-04"));

        let res = matcher.find_match("today is :date").expect("Should match :date");
        assert_eq!(res.snippet.id, "2");
        assert_eq!(res.backspace_count, 5);
    }

    #[test]
    fn test_remove_snippet() {
        let mut matcher = SnippetMatcher::new();
        matcher.add_snippet(Snippet::plain_text("1", "Email", ":mail", "user@example.com"));
        assert!(matcher.find_match(":mail").is_some());

        assert!(matcher.remove_snippet("1"));
        assert!(matcher.find_match(":mail").is_none());
    }
}

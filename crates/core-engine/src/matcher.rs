use crate::snippet::{Snippet, TriggerMode};
use std::{collections::HashMap, sync::Arc};

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
    snippets: Vec<Arc<Snippet>>,
}

/// High-speed snippet matcher using a reverse prefix Trie.
#[derive(Debug, Default, Clone)]
pub struct SnippetMatcher {
    root: Arc<TrieNode>,
    snippets_by_id: Arc<HashMap<String, Arc<Snippet>>>,
}

impl SnippetMatcher {
    /// Creates a new empty `SnippetMatcher`.
    pub fn new() -> Self {
        Self::default()
    }
    pub fn revision(&self) -> usize {
        Arc::as_ptr(&self.root) as usize
    }
    pub fn from_snippets(snippets: impl IntoIterator<Item = Snippet>) -> Self {
        let by_id: HashMap<_, _> = snippets
            .into_iter()
            .map(|s| (s.id.clone(), Arc::new(s)))
            .collect();
        let mut root = TrieNode::default();
        for snippet in by_id
            .values()
            .filter(|s| s.enabled && !s.keyword.is_empty())
        {
            let korean = crate::korean::render(&snippet.keyword);
            for keyword in std::iter::once(snippet.keyword.as_str())
                .chain((korean != snippet.keyword).then_some(korean.as_str()))
            {
                let mut node = &mut root;
                for ch in keyword.chars().rev() {
                    node = node.children.entry(ch).or_default();
                }
                node.snippets.push(Arc::clone(snippet));
            }
        }
        fn sort(node: &mut TrieNode) {
            node.snippets.sort_by(|a, b| a.id.cmp(&b.id));
            for child in node.children.values_mut() {
                sort(child);
            }
        }
        sort(&mut root);
        Self {
            root: Arc::new(root),
            snippets_by_id: Arc::new(by_id),
        }
    }
    pub fn add_snippet(&mut self, snippet: Snippet) {
        let mut rows: Vec<_> = self
            .list_snippets()
            .into_iter()
            .filter(|s| s.id != snippet.id)
            .cloned()
            .collect();
        rows.push(snippet);
        *self = Self::from_snippets(rows);
    }
    pub fn remove_snippet(&mut self, id: &str) -> bool {
        let exists = self.snippets_by_id.contains_key(id);
        if exists {
            *self = Self::from_snippets(
                self.list_snippets()
                    .into_iter()
                    .filter(|s| s.id != id)
                    .cloned(),
            );
        }
        exists
    }
    pub fn list_snippets(&self) -> Vec<&Snippet> {
        self.snippets_by_id.values().map(AsRef::as_ref).collect()
    }
    pub fn get(&self, id: &str) -> Option<&Snippet> {
        self.snippets_by_id.get(id).map(AsRef::as_ref)
    }
    /// Checks if a character is a standard delimiter for Delimiter trigger mode.
    fn is_standard_delimiter(ch: char) -> bool {
        matches!(
            ch,
            ' ' | '\t' | '\r' | '\n' | ',' | '.' | ';' | ':' | '!' | '?' | '/' | '-'
        )
    }

    /// Searches the suffix of the given buffer string for matching snippet keywords.
    /// Returns the best matching snippet and the required backspace count.
    pub fn find_match(&self, buffer: &str) -> Option<MatchResult> {
        self.find_match_in_profile(buffer, "common")
    }
    pub fn find_match_in_profile(&self, buffer: &str, profile: &str) -> Option<MatchResult> {
        let last = buffer.chars().next_back()?;
        let mut best: Option<(&Snippet, usize)> = None;
        for skip in [false, true] {
            if skip && !Self::is_standard_delimiter(last) {
                continue;
            }
            let mut node = self.root.as_ref();
            let mut chars = buffer.chars().rev();
            if skip {
                chars.next();
            }
            for (offset, ch) in chars.enumerate() {
                let Some(next) = node.children.get(&ch) else {
                    break;
                };
                node = next;
                for snippet in &node.snippets {
                    let trigger = match (&snippet.trigger_mode, skip) {
                        (TriggerMode::Immediate, false) => true,
                        (TriggerMode::Space, true) => last == ' ',
                        (TriggerMode::Enter, true) => last == '\n' || last == '\r',
                        (TriggerMode::Tab, true) => last == '\t',
                        (TriggerMode::Delimiter, true) => true,
                        _ => false,
                    };
                    if trigger && (snippet.profile == "common" || snippet.profile == profile) {
                        let length = offset + 1 + usize::from(skip);
                        if best.as_ref().map_or(true, |(s, n)| {
                            length > *n
                                || (length == *n
                                    && s.profile == "common"
                                    && snippet.profile != "common")
                        }) {
                            best = Some((snippet, length));
                        }
                    }
                }
            }
        }
        best.map(|(s, n)| MatchResult {
            snippet: s.clone(),
            backspace_count: n,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::snippet::SnippetContent;
    #[test]
    fn korean_alias_uses_rendered_length_and_preserves_trigger_mode() {
        let mut matcher = SnippetMatcher::from_snippets([
            Snippet::plain_text("1", "alias", "#rksk", "result"),
            Snippet::plain_text("2", "digits", "#123", "digits"),
        ]);
        assert_eq!(matcher.find_match("#rksk").unwrap().backspace_count, 5);
        assert_eq!(matcher.find_match("#가나").unwrap().backspace_count, 3);
        assert!(matcher.find_match("#123").is_some());
        let mut spaced = matcher.get("1").unwrap().clone();
        spaced.trigger_mode = TriggerMode::Space;
        matcher.add_snippet(spaced);
        assert!(matcher.find_match("#가나").is_none());
        assert_eq!(matcher.find_match("#가나 ").unwrap().backspace_count, 4);
        assert_eq!(matcher.find_match("#rksk ").unwrap().backspace_count, 6);
    }
    #[test]
    fn immutable_snapshots_and_profile_scope_preserve_matching() {
        let common = Snippet::plain_text("1", "공통", ":common", "공통");
        let mut work = Snippet::plain_text("2", "업무", ":same", "업무");
        work.profile = "work".into();
        let mut personal = work.clone();
        personal.id = "3".into();
        personal.profile = "personal".into();
        let mut matcher = SnippetMatcher::from_snippets([common, work, personal]);
        let old = matcher.clone();
        assert_eq!(
            matcher
                .find_match_in_profile(":same", "work")
                .unwrap()
                .snippet
                .id,
            "2"
        );
        assert_eq!(
            matcher
                .find_match_in_profile(":same", "personal")
                .unwrap()
                .snippet
                .id,
            "3"
        );
        assert!(matcher.find_match(":same").is_none());
        matcher.remove_snippet("1");
        assert!(old.find_match(":common").is_some());
        assert!(matcher.find_match(":common").is_none());
        let legacy: Snippet = serde_json::from_str(r#"{"id":"old","name":"old","keyword":":old","content":{"PlainText":"문구"},"trigger_mode":"Space","enabled":true}"#).unwrap();
        assert_eq!(legacy.profile, "common");
        assert!(!legacy.template);
    }

    #[test]
    fn test_immediate_match() {
        let mut matcher = SnippetMatcher::new();
        matcher.add_snippet(Snippet::plain_text(
            "1",
            "Email",
            ":mail",
            "user@example.com",
        ));

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
        let match_res = matcher
            .find_match("testing br ")
            .expect("Should match on space");
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
        let match_dot = matcher
            .find_match("hey omw.")
            .expect("Should match on period");
        assert_eq!(match_dot.backspace_count, 4); // "omw" (3) + "." (1)

        let match_comma = matcher
            .find_match("hey omw,")
            .expect("Should match on comma");
        assert_eq!(match_comma.backspace_count, 4);
    }

    #[test]
    fn test_longest_match_priority() {
        let mut matcher = SnippetMatcher::new();
        matcher.add_snippet(Snippet::plain_text("1", "Short", ":d", "short"));
        matcher.add_snippet(Snippet::plain_text("2", "Long", ":date", "2026-11-04"));

        let res = matcher
            .find_match("today is :date")
            .expect("Should match :date");
        assert_eq!(res.snippet.id, "2");
        assert_eq!(res.backspace_count, 5);
    }

    #[test]
    fn test_remove_snippet() {
        let mut matcher = SnippetMatcher::new();
        matcher.add_snippet(Snippet::plain_text(
            "1",
            "Email",
            ":mail",
            "user@example.com",
        ));
        assert!(matcher.find_match(":mail").is_some());

        assert!(matcher.remove_snippet("1"));
        assert!(matcher.find_match(":mail").is_none());
    }

    #[test]
    fn disabled_snippets_remain_editable_and_can_be_enabled_again() {
        let mut matcher = SnippetMatcher::new();
        let mut snippet = Snippet::plain_text("1", "Signature", ":sig", "감사합니다");
        snippet.enabled = false;
        matcher.add_snippet(snippet.clone());
        assert_eq!(matcher.list_snippets().len(), 1);
        assert!(matcher.find_match(":sig").is_none());
        snippet.enabled = true;
        matcher.add_snippet(snippet);
        assert!(matcher.find_match(":sig").is_some());
    }
}

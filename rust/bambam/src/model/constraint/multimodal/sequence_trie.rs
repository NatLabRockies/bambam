use std::collections::HashMap;

/// A trie data structure for storing and searching sequences of strings.
/// Matches prefixes of stored sequences: returns true if the query sequence
/// is a prefix of (or equal to) at least one stored sequence.
#[derive(Debug, Clone)]
pub struct SubSequenceTrie {
    root: TrieNode,
    /// Minimum 1-based depth (leg position) at which each mode appears across all stored sequences.
    min_depth: HashMap<String, usize>,
}

/// Internal node structure for the trie
#[derive(Debug, Clone)]
struct TrieNode {
    /// Children nodes indexed by the string value
    children: HashMap<String, TrieNode>,
}

impl TrieNode {
    fn new() -> Self {
        Self {
            children: HashMap::new(),
        }
    }
}

impl SubSequenceTrie {
    /// Creates a new empty SequenceTrie
    pub fn new() -> Self {
        Self {
            root: TrieNode::new(),
            min_depth: HashMap::new(),
        }
    }

    /// Builds a SequenceTrie from a vector of string sequences
    /// Each sub-vector represents a sequence to be inserted into the trie
    pub fn from_sequences(sequences: Vec<Vec<String>>) -> Self {
        let mut trie = Self::new();
        for sequence in sequences {
            trie.insert_sequence(sequence);
        }
        trie
    }

    /// Inserts a single sequence into the trie
    pub fn insert_sequence(&mut self, sequence: Vec<String>) {
        let mut current_node = &mut self.root;

        for (idx, item) in sequence.into_iter().enumerate() {
            let depth = idx + 1;
            self.min_depth
                .entry(item.clone())
                .and_modify(|min_d| *min_d = (*min_d).min(depth))
                .or_insert(depth);
            current_node = current_node
                .children
                .entry(item)
                .or_insert_with(TrieNode::new);
        }
    }

    /// Searches for a prefix match within the trie.
    /// Returns true if the given sequence is a prefix of (or equal to) any sequence in the trie.
    /// Accepts both &[String] and &[&str]
    pub fn contains<T>(&self, sequence: &[T]) -> bool
    where
        T: AsRef<str>,
    {
        let mut current_node = &self.root;
        for item in sequence {
            match current_node.children.get(item.as_ref()) {
                Some(child) => current_node = child,
                None => return false,
            }
        }
        true
    }

    /// Returns true in O(1) time if `mode` appears at any position within `max_depth` (1-based) in the trie.
    #[inline]
    pub fn contains_mode(&self, mode: &str, max_depth: Option<usize>) -> bool {
        match (self.min_depth.get(mode), max_depth) {
            (Some(&depth), Some(max)) => depth <= max,
            (Some(_), None) => true,
            (None, _) => false,
        }
    }
}

impl Default for SubSequenceTrie {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_trie_is_empty() {
        let trie = SubSequenceTrie::new();
        // Empty trie should not contain any non-empty subsequences
        assert!(!trie.contains(&["A"]));
        // Empty sequence should always be found (empty subsequence is always valid)
        assert!(trie.contains(&[] as &[&str]));
    }

    #[test]
    fn test_insert_single_sequence() {
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec!["A".to_string(), "B".to_string(), "C".to_string()]);

        // Should find exact match
        assert!(trie.contains(&["A", "B", "C"]));

        // Should find valid prefixes
        assert!(trie.contains(&["A"]));
        assert!(trie.contains(&["A", "B"]));

        // Should not find non-prefix subsequences (cannot skip elements)
        assert!(!trie.contains(&["A", "C"]));
        assert!(!trie.contains(&["B", "C"]));
        assert!(!trie.contains(&["B"]));

        // Should not find non-subsequences
        assert!(!trie.contains(&["C", "A"]));
        assert!(!trie.contains(&["B", "A"]));
        assert!(!trie.contains(&["D"]));
    }

    #[test]
    fn test_from_sequences() {
        let sequences = vec![
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            vec!["A".to_string(), "D".to_string(), "E".to_string()],
            vec!["X".to_string(), "Y".to_string()],
        ];

        let trie = SubSequenceTrie::from_sequences(sequences);

        // Should find prefixes from first branch
        assert!(trie.contains(&["A", "B"]));
        assert!(trie.contains(&["A", "B", "C"]));
        assert!(!trie.contains(&["B", "C"]));

        // Should find prefixes from second branch
        assert!(trie.contains(&["A", "D"]));
        assert!(trie.contains(&["A", "D", "E"]));
        assert!(!trie.contains(&["D", "E"]));

        // Should find sequences from third branch
        assert!(trie.contains(&["X", "Y"]));

        // Should find common prefix
        assert!(trie.contains(&["A"]));

        // Should not find cross-branch subsequences
        assert!(!trie.contains(&["B", "D"]));
        assert!(!trie.contains(&["X", "A"]));
    }

    #[test]
    fn test_empty_sequence() {
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec![]);

        // Empty sequence should match empty search
        assert!(trie.contains(&[] as &[&str]));

        // But empty trie should not match non-empty search
        assert!(!trie.contains(&["A"]));
    }

    #[test]
    fn test_single_element_sequences() {
        let sequences = vec![
            vec!["A".to_string()],
            vec!["B".to_string()],
            vec!["C".to_string()],
        ];

        let trie = SubSequenceTrie::from_sequences(sequences);

        assert!(trie.contains(&["A"]));
        assert!(trie.contains(&["B"]));
        assert!(trie.contains(&["C"]));
        assert!(!trie.contains(&["D"]));

        // Multi-element searches should fail
        assert!(!trie.contains(&["A", "B"]));
    }

    #[test]
    fn test_overlapping_sequences() {
        let sequences = vec![
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            vec!["A".to_string(), "B".to_string(), "D".to_string()],
            vec!["A".to_string(), "E".to_string()],
        ];

        let trie = SubSequenceTrie::from_sequences(sequences);

        // Should find common prefixes
        assert!(trie.contains(&["A"]));
        assert!(trie.contains(&["A", "B"]));

        // Should find branch-specific sequences
        assert!(trie.contains(&["A", "B", "C"]));
        assert!(trie.contains(&["A", "B", "D"]));
        assert!(trie.contains(&["A", "E"]));

        // Should not find non-prefix subsequences or cross-branch subsequences
        assert!(!trie.contains(&["B", "C"]));
        assert!(!trie.contains(&["B", "D"]));
        assert!(!trie.contains(&["C", "D"]));
        assert!(!trie.contains(&["E", "C"]));
    }

    #[test]
    fn test_duplicate_sequences() {
        let sequences = vec![
            vec!["A".to_string(), "B".to_string()],
            vec!["A".to_string(), "B".to_string()], // Duplicate
            vec!["C".to_string(), "D".to_string()],
        ];

        let trie = SubSequenceTrie::from_sequences(sequences);

        // Should still work correctly with duplicates
        assert!(trie.contains(&["A", "B"]));
        assert!(trie.contains(&["C", "D"]));
        assert!(trie.contains(&["A"]));
        assert!(!trie.contains(&["A", "C"]));
    }

    #[test]
    fn test_long_subsequence_matching() {
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec![
            "A".to_string(),
            "B".to_string(),
            "C".to_string(),
            "D".to_string(),
            "E".to_string(),
            "F".to_string(),
        ]);

        // Test various prefix lengths
        assert!(trie.contains(&["A"]));
        assert!(trie.contains(&["A", "B"]));
        assert!(trie.contains(&["A", "B", "C", "D"]));
        assert!(trie.contains(&["A", "B", "C", "D", "E", "F"]));

        // Non-prefix subsequences should not match
        assert!(!trie.contains(&["A", "C"]));
        assert!(!trie.contains(&["A", "C", "E"]));
        assert!(!trie.contains(&["B", "D", "F"]));

        // Test order preservation
        assert!(!trie.contains(&["C", "A"]));
        assert!(!trie.contains(&["F", "E"]));

        // Test non-existent elements
        assert!(!trie.contains(&["A", "G"]));
    }

    #[test]
    fn test_case_sensitivity() {
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec!["Hello".to_string(), "World".to_string()]);

        assert!(trie.contains(&["Hello"]));
        assert!(trie.contains(&["Hello", "World"]));

        // Should be case sensitive
        assert!(!trie.contains(&["hello"]));
        assert!(!trie.contains(&["HELLO"]));
    }

    #[test]
    fn test_default_implementation() {
        let trie: SubSequenceTrie = Default::default();
        assert!(!trie.contains(&["A"]));
    }

    #[test]
    fn test_nested_sequences_still_work_correctly() {
        // This test demonstrates that prefix matching works correctly
        // when one sequence is a prefix of another
        let sequences = vec![
            vec!["A".to_string(), "B".to_string()], // Short sequence
            vec!["A".to_string(), "B".to_string(), "C".to_string()], // Extended sequence
        ];

        let trie = SubSequenceTrie::from_sequences(sequences);

        // Both sequences should be found
        assert!(trie.contains(&["A", "B"]));
        assert!(trie.contains(&["A", "B", "C"]));

        // Prefixes should be found, non-prefixes should not
        assert!(trie.contains(&["A"]));
        assert!(!trie.contains(&["B"]));
        assert!(!trie.contains(&["A", "C"]));
        assert!(!trie.contains(&["B", "C"]));
    }

    #[test]
    fn test_accepts_both_string_and_str_slices() {
        let mut trie = SubSequenceTrie::new();
        trie.insert_sequence(vec![
            "Hello".to_string(),
            "World".to_string(),
            "Test".to_string(),
        ]);

        // Test with &[String] (keeping these to demonstrate the generic capability)
        assert!(trie.contains(&["Hello".to_string(), "World".to_string()]));
        assert!(trie.contains(&[
            "Hello".to_string(),
            "World".to_string(),
            "Test".to_string()
        ]));

        // Test with &[&str]
        assert!(trie.contains(&["Hello", "World"]));
        assert!(trie.contains(&["Hello", "World", "Test"]));
        assert!(!trie.contains(&["Hello", "Test"]));
        assert!(!trie.contains(&["World", "Test"]));

        // Test mixed scenarios
        let string_vec = vec!["Hello".to_string()];
        let str_slice = ["World"];
        assert!(trie.contains(&string_vec));
        assert!(!trie.contains(&str_slice));

        // Test non-matches with both types
        assert!(!trie.contains(&["World", "Hello"])); // Wrong order
        assert!(!trie.contains(&["Missing"]));
    }
}

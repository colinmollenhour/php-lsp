//! Fuzzy matching for completion and workspace-symbol filtering.

/// A fuzzy-match query with its lowercase form computed once.
///
/// Matching loops over thousands of candidates (workspace symbols, completion
/// filtering) previously lowercased the query — and the candidate, into fresh
/// `String`s — per candidate. Build a `FuzzyQuery` once per request instead;
/// the per-candidate checks below allocate nothing (char-iterator comparisons,
/// Unicode-lowercase-correct like the old code).
pub(crate) struct FuzzyQuery {
    lower: String,
}

impl FuzzyQuery {
    pub(crate) fn new(query: &str) -> Self {
        FuzzyQuery {
            lower: query.to_lowercase(),
        }
    }

    /// Returns `true` if the query matches `candidate` using
    /// camelCase/underscore abbreviation rules (no substring fallback).
    ///
    /// Rules (applied in order, first match wins):
    /// 1. `candidate` starts with the query (case-insensitive prefix match).
    /// 2. Every character of the query matches a camelCase word boundary or
    ///    character after `_` / `$` in the candidate.
    ///
    /// Examples:
    /// - `"GRF"` matches `"getRecentFiles"`
    /// - `"str_r"` matches `"str_replace"` (boundary after `_`)
    ///
    /// See [`Self::symbol_match`] for the variant that also accepts
    /// substrings, which is appropriate for workspace symbol search but not
    /// for completions.
    pub(crate) fn camel_match(&self, candidate: &str) -> bool {
        if self.lower.is_empty() {
            return true;
        }
        // Rule 1: plain prefix
        if starts_with_at(candidate, &self.lower, 0) {
            return true;
        }
        // Rule 2: camel / underscore abbreviation
        self.camel_abbrev(candidate)
    }

    /// Like [`Self::camel_match`] but also accepts contiguous substrings as a
    /// fallback.  Use for workspace symbol search (where "Controller" should
    /// match "BlogController") but NOT for completions (substring produces too
    /// many hits).
    pub(crate) fn symbol_match(&self, candidate: &str) -> bool {
        if self.camel_match(candidate) {
            return true;
        }
        // Substring fallback: "Controller" matches "BlogController".
        // char_indices keeps the scan allocation-free; candidates are short
        // identifiers, so the O(n·m) window walk beats two String allocations.
        candidate
            .char_indices()
            .any(|(i, _)| starts_with_at(candidate, &self.lower, i))
    }

    /// Core camel/underscore abbreviation check against the lowercased query.
    fn camel_abbrev(&self, candidate: &str) -> bool {
        let mut query = self.lower.chars().peekable();
        let mut prev: Option<char> = None;
        for cc in candidate.chars() {
            let Some(&qc) = query.peek() else {
                return true;
            };
            // A "word boundary" in the candidate is: position 0, after '_' or
            // '$', or an uppercase letter after a lowercase letter (camelCase
            // transition).
            let is_boundary = match prev {
                None => true,
                Some('_') | Some('$') => true,
                Some(p) => cc.is_uppercase() && p.is_lowercase(),
            };
            if is_boundary && cc.to_lowercase().next() == Some(qc) {
                query.next();
            }
            prev = Some(cc);
        }
        query.peek().is_none()
    }
}

/// Case-insensitive (Unicode-lowercase) test that `candidate[at..]` starts
/// with the already-lowercased `query_lower`, without allocating.
fn starts_with_at(candidate: &str, query_lower: &str, at: usize) -> bool {
    let mut c = candidate[at..].chars().flat_map(char::to_lowercase);
    let mut q = query_lower.chars();
    loop {
        match (q.next(), c.next()) {
            (None, _) => return true,
            (Some(qc), Some(cc)) if qc == cc => continue,
            _ => return false,
        }
    }
}

/// One-shot wrapper around [`FuzzyQuery::camel_match`]. Prefer building a
/// [`FuzzyQuery`] outside the loop when matching many candidates.
pub(crate) fn fuzzy_camel_match(query: &str, candidate: &str) -> bool {
    FuzzyQuery::new(query).camel_match(candidate)
}

/// One-shot wrapper around [`FuzzyQuery::symbol_match`]. Prefer building a
/// [`FuzzyQuery`] outside the loop when matching many candidates.
pub(crate) fn fuzzy_symbol_match(query: &str, candidate: &str) -> bool {
    FuzzyQuery::new(query).symbol_match(candidate)
}

/// Compute a sort key so prefix matches sort before camel-abbreviation matches.
/// Lower string = higher priority.  Only called on items that passed
/// [`fuzzy_camel_match`], so the `else` branch (substring) is unreachable here.
pub(crate) fn camel_sort_key(query: &str, label: &str) -> String {
    let lq = query.to_lowercase();
    let ll = label.to_lowercase();
    if ll.starts_with(&lq) {
        format!("0{}", ll)
    } else {
        format!("1{}", ll)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fuzzy_camel_match_prefix() {
        assert!(fuzzy_camel_match("Blog", "BlogController"));
        assert!(fuzzy_camel_match("blog", "BlogController"));
    }

    #[test]
    fn fuzzy_camel_match_abbreviation() {
        assert!(fuzzy_camel_match("BC", "BlogController"));
        assert!(fuzzy_camel_match("GRF", "getRecentFiles"));
        assert!(fuzzy_camel_match("str_r", "str_replace")); // boundary after '_'
    }

    #[test]
    fn fuzzy_camel_match_no_substring() {
        // fuzzy_camel_match (used for completions) must NOT match substrings
        assert!(!fuzzy_camel_match("Controller", "BlogController"));
        assert!(!fuzzy_camel_match("xyz", "BlogController"));
    }

    #[test]
    fn fuzzy_symbol_match_substring_fallback() {
        // fuzzy_symbol_match (used for workspace symbols) DOES match substrings
        assert!(fuzzy_symbol_match("Controller", "BlogController"));
        assert!(fuzzy_symbol_match("controller", "BlogController"));
        assert!(fuzzy_symbol_match("controller", "UserController"));
        // prefix and camel still work
        assert!(fuzzy_symbol_match("Blog", "BlogController"));
        assert!(fuzzy_symbol_match("BC", "BlogController"));
        // no match
        assert!(!fuzzy_symbol_match("xyz", "BlogController"));
    }
}

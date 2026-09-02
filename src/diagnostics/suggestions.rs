//! Levenshtein distance and fuzzy typo suggestion engine for Datara diagnostics.

/// Computes standard Levenshtein edit distance between two UTF-8 strings.
pub fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();

    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev_row: Vec<usize> = (0..=n).collect();
    let mut curr_row: Vec<usize> = vec![0; n + 1];

    for i in 1..=m {
        curr_row[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr_row[j] = (prev_row[j] + 1)
                .min(curr_row[j - 1] + 1)
                .min(prev_row[j - 1] + cost);
        }
        prev_row.copy_from_slice(&curr_row);
    }

    prev_row[n]
}

/// Computes normalized similarity score between 0.0 (completely distinct) and 1.0 (exact match).
pub fn similarity_score(a: &str, b: &str) -> f64 {
    let max_len = a.len().max(b.len());
    if max_len == 0 {
        return 1.0;
    }
    let dist = levenshtein_distance(a, b);
    1.0 - (dist as f64 / max_len as f64)
}

/// Finds the most likely candidate from a list of candidate names.
/// Returns Some(best_match) if the match is sufficiently close, None otherwise.
pub fn find_best_match<'a, I>(needle: &str, candidates: I) -> Option<&'a str>
where
    I: IntoIterator<Item = &'a str>,
{
    if needle.is_empty() {
        return None;
    }

    let needle_lower = needle.to_lowercase();
    let needle_len = needle.len();

    // Determine max allowed distance based on word length
    let max_allowed_dist = match needle_len {
        0..=3 => 1,
        4..=8 => 2,
        _ => 3,
    };

    let mut best_candidate: Option<&'a str> = None;
    let mut min_distance = usize::MAX;
    let mut highest_similarity = 0.0f64;

    for candidate in candidates {
        if candidate.is_empty() || candidate == needle {
            continue;
        }

        // Exact case-insensitive match is an immediate winner
        if candidate.to_lowercase() == needle_lower {
            return Some(candidate);
        }

        let dist = levenshtein_distance(&needle_lower, &candidate.to_lowercase());
        let sim = similarity_score(&needle_lower, &candidate.to_lowercase());

        // Check if candidate is a prefix or suffix with minor variation
        let is_prefix_or_suffix = candidate.starts_with(needle) || needle.starts_with(candidate);

        if (dist <= max_allowed_dist || (is_prefix_or_suffix && dist <= max_allowed_dist + 1) || sim >= 0.68)
            && dist < min_distance
        {
            min_distance = dist;
            highest_similarity = sim;
            best_candidate = Some(candidate);
        }
    }

    if highest_similarity >= 0.50 || min_distance <= max_allowed_dist {
        best_candidate
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("kitten", "sitting"), 3);
        assert_eq!(levenshtein_distance("hello", "hello"), 0);
        assert_eq!(levenshtein_distance("var", "var_"), 1);
        assert_eq!(levenshtein_distance("count", "countr"), 1);
    }

    #[test]
    fn test_find_best_match() {
        let candidates = ["counter", "total_sum", "item_count", "user_id"];
        assert_eq!(find_best_match("countr", candidates.iter().copied()), Some("counter"));
        assert_eq!(find_best_match("totl_sum", candidates.iter().copied()), Some("total_sum"));
        assert_eq!(find_best_match("completely_unrelated", candidates.iter().copied()), None);
    }

    #[test]
    fn test_case_insensitive_match() {
        let candidates = ["myVariable", "my_variable"];
        assert_eq!(find_best_match("myvariable", candidates.iter().copied()), Some("myVariable"));
    }
}

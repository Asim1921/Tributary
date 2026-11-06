//! Wildcard patterns, used for site rules.

/// Matches text against a pattern where `*` is any run of characters and `?` is one
/// character. Case doesn't matter.
pub fn wildcard_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let text: Vec<char> = text.to_lowercase().chars().collect();

    let (mut p, mut t) = (0, 0);
    let mut star: Option<(usize, usize)> = None;

    while t < text.len() {
        match pattern.get(p) {
            Some('*') => {
                star = Some((p, t));
                p += 1;
            }
            Some(&c) if c == '?' || c == text[t] => {
                p += 1;
                t += 1;
            }
            _ => match star {
                // Lets the last `*` take one more character, and tries again from there.
                Some((star_p, star_t)) => {
                    p = star_p + 1;
                    t = star_t + 1;
                    star = Some((star_p, star_t + 1));
                }
                None => return false,
            },
        }
    }

    pattern[p..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards_match_like_a_person_would_expect() {
        let cases = [
            ("*.zip", "report.zip", true),
            ("*.zip", "report.zip.txt", false),
            ("REPORT*", "report-final.pdf", true),
            ("file?.txt", "file1.txt", true),
            ("file?.txt", "file12.txt", false),
            ("*a*b*c*", "xxaxxbxxcxx", true),
            ("*a*b*c*", "xxaxxcxxbxx", false),
            ("*", "", true),
            ("", "", true),
            ("", "x", false),
            ("a*", "a", true),
            ("exact", "exact", true),
            ("exact", "exactly", false),
        ];
        for (pattern, text, expected) in cases {
            assert_eq!(
                wildcard_match(pattern, text),
                expected,
                "{pattern} vs {text}"
            );
        }
    }

    #[test]
    fn repeated_stars_do_not_take_forever() {
        let text = "a".repeat(2000);
        assert!(!wildcard_match("*a*a*a*a*a*a*a*a*b", &text));
    }
}

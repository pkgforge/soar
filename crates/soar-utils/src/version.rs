//! Ordering for the version strings repositories actually publish.
//!
//! Comparison is segment-wise: a version is split into runs of digits and runs
//! of non-digits, and matching runs are compared numerically when both are
//! numeric and lexically otherwise. This avoids the two ways a plain string
//! comparison gets it wrong, `10` sorting below `9` and `1.10` below `1.9`.
//!
//! Strict semver is deliberately not used. Most published versions carry a
//! rebuild revision as `-N`, which semver reads as a prerelease and therefore
//! ranks *below* the plain version, the opposite of what it means here. Others
//! (`1.05`, `7.1-2`, `r1287.fef2b38-1`) are not valid semver at all.
//!
//! A version built from a commit hash has no order to recover: hashes carry no
//! time. Those compare equal to each other and below any ordinary version, so a
//! repository that wants upgrades between snapshots has to publish something
//! ordered, such as a date.

use std::cmp::Ordering;

/// Compare two version strings.
///
/// ```
/// use std::cmp::Ordering;
/// use soar_utils::version::compare_versions;
///
/// assert_eq!(compare_versions("10.4.2", "9.0.0"), Ordering::Greater);
/// assert_eq!(compare_versions("1.10.0", "1.9.0"), Ordering::Greater);
/// assert_eq!(compare_versions("1.14.0-2", "1.14.0-1"), Ordering::Greater);
/// assert_eq!(compare_versions("2026.05.24", "2026.05.23"), Ordering::Greater);
/// ```
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    // Two commit hashes carry no order at all, and segment rules would invent
    // one, letting an arbitrary hash read as an upgrade or a downgrade. A hash
    // ranks below any ordinary version rather than being compared segment-wise
    // against one, which would leave the ordering intransitive: two hashes
    // equal to each other yet landing on opposite sides of the same tag, which
    // is enough to panic `sort_by`.
    match (is_commit_hash(a), is_commit_hash(b)) {
        (true, true) => return Ordering::Equal,
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (false, false) => {}
    }

    let mut left = segments(a);
    let mut right = segments(b);

    loop {
        match (left.next(), right.next()) {
            (None, None) => return Ordering::Equal,
            // A trailing segment means opposite things depending on its
            // shape: a number is a further release (1.2.1 over 1.2, or the
            // rebuild in 1.14.0-1 over 1.14.0), while text is a prerelease
            // (1.2rc and 0.5.7-beta both precede their release).
            (Some(x), None) => return extra_segment_order(x),
            (None, Some(y)) => return extra_segment_order(y).reverse(),
            (Some(x), Some(y)) => {
                match compare_segment(x, y) {
                    Ordering::Equal => continue,
                    other => return other,
                }
            }
        }
    }
}

/// Whether `candidate` supersedes `current`.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    compare_versions(candidate, current) == Ordering::Greater
}

/// Whether a version is nothing but a commit hash.
///
/// Requires a letter, so a long run of digits stays a version: 20260412 is a
/// date, not a hash.
fn is_commit_hash(version: &str) -> bool {
    version.len() >= 7
        && version.len() <= 40
        && version.chars().all(|c| c.is_ascii_hexdigit())
        && version.chars().any(|c| c.is_ascii_alphabetic())
}

/// How a version compares against one that stopped earlier, judged by the
/// first segment the longer one carries alone.
fn extra_segment_order(segment: &str) -> Ordering {
    if segment.starts_with(|c: char| c.is_ascii_digit()) {
        Ordering::Greater
    } else {
        Ordering::Less
    }
}

fn compare_segment(a: &str, b: &str) -> Ordering {
    match (a.parse::<u64>(), b.parse::<u64>()) {
        (Ok(x), Ok(y)) => x.cmp(&y),
        // A numeric segment outranks a textual one, so 1.2 beats 1.2rc.
        (Ok(_), Err(_)) => Ordering::Greater,
        (Err(_), Ok(_)) => Ordering::Less,
        (Err(_), Err(_)) => a.cmp(b),
    }
}

/// Split a version into runs of digits and runs of everything else,
/// discarding the separators between them.
fn segments(version: &str) -> impl Iterator<Item = &str> {
    let mut rest = version;
    std::iter::from_fn(move || {
        while let Some(c) = rest.chars().next() {
            if c.is_ascii_alphanumeric() {
                break;
            }
            rest = &rest[c.len_utf8()..];
        }
        if rest.is_empty() {
            return None;
        }
        let numeric = rest.starts_with(|c: char| c.is_ascii_digit());
        let end = rest
            .find(|c: char| c.is_ascii_digit() != numeric || !c.is_ascii_alphanumeric())
            .unwrap_or(rest.len());
        let (seg, tail) = rest.split_at(end);
        rest = tail;
        Some(seg)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_segments_beat_string_order() {
        assert_eq!(compare_versions("10.4.2", "9.0.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.10.0", "1.9.0"), Ordering::Greater);
        assert_eq!(compare_versions("1.06", "1.05"), Ordering::Greater);
    }

    #[test]
    fn rebuild_revision_is_newer_not_older() {
        // semver would call these prereleases and rank them below 1.14.0
        assert_eq!(compare_versions("1.14.0-1", "1.14.0"), Ordering::Greater);
        assert_eq!(compare_versions("2.0.19-4", "2.0.19-3"), Ordering::Greater);
        assert_eq!(compare_versions("0.2.4-4", "0.2.4-3"), Ordering::Greater);
    }

    #[test]
    fn dates_order_naturally() {
        assert_eq!(
            compare_versions("2026.05.24", "2026.05.23"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("2026.05.24.1.dda726e-1", "2026.05.23.1.aaa111b-1"),
            Ordering::Greater
        );
    }

    #[test]
    fn dates_compare_whatever_separates_them() {
        // hyphen-separated, the ISO form
        assert_eq!(
            compare_versions("2026-04-12", "2026-04-11"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("2026-05-01", "2026-04-30"),
            Ordering::Greater
        );
        assert_eq!(
            compare_versions("2027-01-01", "2026-12-31"),
            Ordering::Greater
        );
        // leading zeros are numeric, not text
        assert_eq!(compare_versions("2026-04-09", "2026-04-10"), Ordering::Less);
        // the separator itself carries no meaning
        assert_eq!(
            compare_versions("2026-04-12", "2026.04.12"),
            Ordering::Equal
        );
        assert_eq!(
            compare_versions("2026-04-12-2", "2026-04-12"),
            Ordering::Greater
        );
    }

    #[test]
    fn equal_versions_compare_equal() {
        assert_eq!(compare_versions("1.2.3", "1.2.3"), Ordering::Equal);
        assert_eq!(compare_versions("89c99d2a9", "89c99d2a9"), Ordering::Equal);
    }

    #[test]
    fn commit_hashes_have_no_meaningful_order() {
        // Neither supersedes the other, so neither can drive an upgrade.
        assert_eq!(compare_versions("89c99d2a9", "0f3a21b"), Ordering::Equal);
        assert_eq!(compare_versions("0f3a21b", "89c99d2a9"), Ordering::Equal);
        assert!(!is_newer("89c99d2a9", "0f3a21b"));
        assert!(!is_newer("0f3a21b", "89c99d2a9"));
    }

    #[test]
    fn hashes_rank_below_ordinary_versions() {
        // Sorting a mixed list needs a total order. Comparing each hash
        // segment-wise against the tag would put one above it and one below,
        // while the two stay equal to each other.
        assert_eq!(compare_versions("0f3a21b", "5.0"), Ordering::Less);
        assert_eq!(compare_versions("89c99d2a9", "5.0"), Ordering::Less);
        assert_eq!(compare_versions("5.0", "89c99d2a9"), Ordering::Greater);

        let mut versions = ["89c99d2a9", "5.0", "0f3a21b", "4.9"];
        versions.sort_by(|a, b| compare_versions(b, a));
        assert_eq!(&versions[..2], &["5.0", "4.9"]);
    }

    #[test]
    fn digit_runs_are_versions_not_hashes() {
        // a date-like version must keep comparing normally
        assert_eq!(compare_versions("20260413", "20260412"), Ordering::Greater);
    }

    #[test]
    fn longer_version_wins_when_prefix_matches() {
        assert_eq!(compare_versions("1.2.1", "1.2"), Ordering::Greater);
        assert_eq!(compare_versions("3.13.1", "3.13"), Ordering::Greater);
    }

    #[test]
    fn text_segments_rank_below_numeric() {
        // a text suffix is a prerelease and precedes the plain version
        assert_eq!(compare_versions("1.2", "1.2rc"), Ordering::Greater);
        assert_eq!(compare_versions("0.5.7", "0.5.7-beta"), Ordering::Greater);
        // between two prereleases, order is lexical
        assert_eq!(
            compare_versions("0.5.7-beta", "0.5.7-alpha"),
            Ordering::Greater
        );
    }
}

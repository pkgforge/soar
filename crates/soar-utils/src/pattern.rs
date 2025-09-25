/// Applies the `.sig` variant to a list of patterns.
///
/// This function takes a list of patterns and appends the `.sig` variant to
/// each pattern. If the pattern starts with `!`, the pattern is negated.
///
/// # Arguments
/// * `patterns` - A vector of patterns to apply the `.sig` variant to.
///
/// # Returns
/// A vector of patterns with the `.sig` variant applied.
///
/// # Examples
///
/// ```
/// use soar_utils::pattern::apply_sig_variants;
///
/// let patterns = vec!["foo", "!bar", "baz"]
///     .into_iter()
///     .map(String::from)
///     .collect();
/// let sig_variants = apply_sig_variants(patterns);
///
/// assert_eq!(sig_variants, vec!["{foo,foo.sig}", "!{bar,bar.sig}", "{baz,baz.sig}"]);
/// ```
pub fn apply_sig_variants(patterns: Vec<String>) -> Vec<String> {
    patterns
        .into_iter()
        .map(|pat| {
            let (negate, inner) = if let Some(rest) = pat.strip_prefix('!') {
                (true, rest)
            } else {
                (false, pat.as_str())
            };

            let sig_variant = format!("{inner}.sig");
            let brace_pattern = format!("{{{inner},{sig_variant}}}");

            if negate {
                format!("!{brace_pattern}")
            } else {
                brace_pattern
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_apply_sig_variants() {
        let patterns = vec!["foo", "!bar", "baz"]
            .into_iter()
            .map(String::from)
            .collect();
        let sig_variants = apply_sig_variants(patterns);

        assert_eq!(
            sig_variants,
            vec!["{foo,foo.sig}", "!{bar,bar.sig}", "{baz,baz.sig}"]
        );
    }
}

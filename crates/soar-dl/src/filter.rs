use fast_glob::glob_match;
use regex::Regex;

#[derive(Debug, Clone, Default)]
pub struct Filter {
    pub regexes: Vec<Regex>,
    pub globs: Vec<String>,
    pub include: Vec<String>,
    pub exclude: Vec<String>,
    pub case_sensitive: bool,
}

impl Filter {
    /// Determines whether a name satisfies this filter's combined criteria.
    ///
    /// The name must match every regex in `self.regexes`, match at least one glob in
    /// `self.globs`, satisfy all include keyword groups in `self.include`, and must
    /// not match any exclude keyword groups in `self.exclude`.
    ///
    /// # Returns
    ///
    /// `true` if the name matches all regexes, at least one glob, all include groups,
    /// and no exclude groups; `false` otherwise.
    ///
    /// # Examples
    ///
    /// ```
    /// let f = Filter {
    ///     regexes: Vec::new(),
    ///     globs: vec!["*".into()],
    ///     include: Vec::new(),
    ///     exclude: Vec::new(),
    ///     case_sensitive: true,
    /// };
    /// assert!(f.matches("anything"));
    /// ```
    pub fn matches(&self, name: &str) -> bool {
        let matches_regex = self.regexes.iter().all(|r| r.is_match(name));
        let matches_glob = self.globs.iter().any(|g| glob_match(g, name));
        let matches_include = self.matches_keywords(name, &self.include, true);
        let matches_exclude = !self.matches_keywords(name, &self.exclude, false);

        matches_regex && matches_glob && matches_include && matches_exclude
    }

    /// Determines whether every keyword group in `keywords` satisfies the required presence or absence
    /// against `name` according to `must_match`.
    ///
    /// - If `keywords` is empty, returns `true`.
    /// - Splits each keyword string on commas, trims parts, and ignores empty parts.
    /// - Respects `case_sensitive`: comparisons use the original case when `true`, otherwise both
    ///   haystack and needles are lowercased.
    /// - For each keyword (a group of comma-separated alternatives), any one alternative matching
    ///   `name` counts as a match for that keyword.
    /// - If `must_match` is `true`, each keyword group must have at least one matching alternative.
    ///   If `must_match` is `false`, each keyword group must have no matching alternatives.
    ///
    /// # Examples
    ///
    /// ```
    /// use regex::Regex;
    /// use crate::filter::Filter;
    ///
    /// let filter = Filter {
    ///     regexes: vec![],
    ///     globs: vec![],
    ///     include: vec!["foo,bar".to_string()],
    ///     exclude: vec![],
    ///     case_sensitive: false,
    /// };
    ///
    /// // "barbaz" contains "bar", one of the alternatives in the include group.
    /// assert!(filter.matches("barbaz"));
    /// ```
    fn matches_keywords(&self, name: &str, keywords: &[String], must_match: bool) -> bool {
        if keywords.is_empty() {
            return true;
        }

        let haystack = if self.case_sensitive {
            name.to_string()
        } else {
            name.to_lowercase()
        };

        keywords.iter().all(|kw| {
            let parts: Vec<_> = kw
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();

            let any_match = parts.iter().any(|&part| {
                let needle = if self.case_sensitive {
                    part.to_string()
                } else {
                    part.to_lowercase()
                };
                haystack.contains(&needle)
            });

            if must_match {
                any_match
            } else {
                !any_match
            }
        })
    }
}
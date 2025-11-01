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
    /// use soar_dl::filter::Filter;
    ///
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
        let matches_regex =
            self.regexes.is_empty() || self.regexes.iter().all(|r| r.is_match(name));
        let matches_glob = self.globs.is_empty()
            || if self.case_sensitive {
                self.globs.iter().any(|g| glob_match(g, name))
            } else {
                self.globs
                    .iter()
                    .any(|g| glob_match(g.to_lowercase(), name.to_lowercase()))
            };
        let matches_include = self.matches_keywords(name, &self.include, true);
        let matches_exclude = self.matches_keywords(name, &self.exclude, false);

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
    /// use soar_dl::filter::Filter;
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

#[cfg(test)]
mod tests {
    use regex::Regex;

    use super::*;

    #[test]
    fn test_filter_default() {
        let filter = Filter::default();
        assert!(filter.regexes.is_empty());
        assert!(filter.globs.is_empty());
        assert!(filter.include.is_empty());
        assert!(filter.exclude.is_empty());
        assert!(!filter.case_sensitive);
    }

    #[test]
    fn test_matches_empty_filter() {
        let filter = Filter::default();
        // Empty filter should match everything
        assert!(filter.matches("anything"));
        assert!(filter.matches(""));
        assert!(filter.matches("test.tar.gz"));
    }

    #[test]
    fn test_matches_regex() {
        let filter = Filter {
            regexes: vec![Regex::new(r"\.tar\.gz$").unwrap()],
            globs: vec![],
            include: vec![],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("archive.tar.gz"));
        assert!(filter.matches("file-v1.0.tar.gz"));
        assert!(!filter.matches("archive.zip"));
        assert!(!filter.matches("file.tar"));
    }

    #[test]
    fn test_matches_multiple_regexes() {
        let filter = Filter {
            regexes: vec![Regex::new(r"^file").unwrap(), Regex::new(r"linux").unwrap()],
            globs: vec![],
            include: vec![],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("file-linux-x86_64"));
        assert!(!filter.matches("archive-linux-x86_64")); // doesn't start with "file"
        assert!(!filter.matches("file-windows-x86_64")); // doesn't contain "linux"
    }

    #[test]
    fn test_matches_glob_case_sensitive() {
        let filter = Filter {
            regexes: vec![],
            globs: vec!["*.tar.gz".to_string()],
            include: vec![],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("archive.tar.gz"));
        assert!(filter.matches("file.tar.gz"));
        assert!(!filter.matches("archive.TAR.GZ"));
        assert!(!filter.matches("archive.zip"));
    }

    #[test]
    fn test_matches_glob_case_insensitive() {
        let filter = Filter {
            regexes: vec![],
            globs: vec!["*.tar.gz".to_string()],
            include: vec![],
            exclude: vec![],
            case_sensitive: false,
        };

        assert!(filter.matches("archive.tar.gz"));
        assert!(filter.matches("archive.TAR.GZ"));
        assert!(filter.matches("file.Tar.Gz"));
        assert!(!filter.matches("archive.zip"));
    }

    #[test]
    fn test_matches_multiple_globs() {
        let filter = Filter {
            regexes: vec![],
            globs: vec!["*.tar.gz".to_string(), "*.zip".to_string()],
            include: vec![],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("archive.tar.gz"));
        assert!(filter.matches("file.zip"));
        assert!(!filter.matches("file.tar"));
        assert!(!filter.matches("file.7z"));
    }

    #[test]
    fn test_matches_include_single_keyword() {
        let filter = Filter {
            regexes: vec![],
            globs: vec![],
            include: vec!["linux".to_string()],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("file-linux-x86_64"));
        assert!(filter.matches("linux-binary"));
        assert!(!filter.matches("file-windows-x86_64"));
        assert!(!filter.matches("darwin-binary"));
    }

    #[test]
    fn test_matches_include_multiple_keywords() {
        let filter = Filter {
            regexes: vec![],
            globs: vec![],
            include: vec!["linux".to_string(), "x86_64".to_string()],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("file-linux-x86_64"));
        assert!(!filter.matches("file-linux-arm64")); // missing x86_64
        assert!(!filter.matches("file-darwin-x86_64")); // missing linux
    }

    #[test]
    fn test_matches_include_alternatives() {
        let filter = Filter {
            regexes: vec![],
            globs: vec![],
            include: vec!["linux,darwin".to_string()],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("file-linux-x86_64"));
        assert!(filter.matches("file-darwin-x86_64"));
        assert!(!filter.matches("file-windows-x86_64"));
    }

    #[test]
    fn test_matches_include_case_insensitive() {
        let filter = Filter {
            regexes: vec![],
            globs: vec![],
            include: vec!["Linux".to_string()],
            exclude: vec![],
            case_sensitive: false,
        };

        assert!(filter.matches("file-linux-x86_64"));
        assert!(filter.matches("file-LINUX-x86_64"));
        assert!(filter.matches("file-Linux-x86_64"));
    }

    #[test]
    fn test_matches_exclude_single_keyword() {
        let filter = Filter {
            regexes: vec![],
            globs: vec![],
            include: vec![],
            exclude: vec!["debug".to_string()],
            case_sensitive: true,
        };

        assert!(filter.matches("file-release"));
        assert!(!filter.matches("file-debug"));
        assert!(!filter.matches("debug-symbols"));
    }

    #[test]
    fn test_matches_exclude_multiple_keywords() {
        let filter = Filter {
            regexes: vec![],
            globs: vec![],
            include: vec![],
            exclude: vec!["debug".to_string(), "test".to_string()],
            case_sensitive: true,
        };

        assert!(filter.matches("file-release"));
        assert!(!filter.matches("file-debug"));
        assert!(!filter.matches("test-binary"));
        assert!(!filter.matches("debug-test-binary"));
    }

    #[test]
    fn test_matches_exclude_alternatives() {
        let filter = Filter {
            regexes: vec![],
            globs: vec![],
            include: vec![],
            exclude: vec!["debug,test".to_string()],
            case_sensitive: true,
        };

        assert!(filter.matches("file-release"));
        assert!(!filter.matches("file-debug"));
        assert!(!filter.matches("file-test"));
    }

    #[test]
    fn test_matches_combined_filters() {
        let filter = Filter {
            regexes: vec![Regex::new(r"^file").unwrap()],
            globs: vec!["*.tar.gz".to_string()],
            include: vec!["linux".to_string(), "x86_64".to_string()],
            exclude: vec!["debug".to_string()],
            case_sensitive: true,
        };

        assert!(filter.matches("file-linux-x86_64-v1.0.tar.gz"));
        assert!(!filter.matches("archive-linux-x86_64-v1.0.tar.gz")); // doesn't start with "file"
        assert!(!filter.matches("file-linux-x86_64-v1.0.zip")); // wrong extension
        assert!(!filter.matches("file-darwin-x86_64-v1.0.tar.gz")); // not linux
        assert!(!filter.matches("file-linux-arm64-v1.0.tar.gz")); // not x86_64
        assert!(!filter.matches("file-linux-x86_64-debug.tar.gz")); // contains "debug"
    }

    #[test]
    fn test_matches_keywords_empty() {
        let filter = Filter::default();
        assert!(filter.matches_keywords("anything", &[], true));
        assert!(filter.matches_keywords("anything", &[], false));
    }

    #[test]
    fn test_matches_keywords_whitespace_handling() {
        let filter = Filter {
            regexes: vec![],
            globs: vec![],
            include: vec!["  linux  ,  darwin  ".to_string()],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("file-linux-x86_64"));
        assert!(filter.matches("file-darwin-x86_64"));
    }

    #[test]
    fn test_matches_keywords_empty_alternatives() {
        let filter = Filter {
            regexes: vec![],
            globs: vec![],
            include: vec!["linux,,darwin".to_string()],
            exclude: vec![],
            case_sensitive: true,
        };

        // Empty alternatives should be filtered out
        assert!(filter.matches("file-linux-x86_64"));
        assert!(filter.matches("file-darwin-x86_64"));
    }

    #[test]
    fn test_glob_wildcard_patterns() {
        let filter = Filter {
            regexes: vec![],
            globs: vec!["file-*-x86_64".to_string()],
            include: vec![],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("file-linux-x86_64"));
        assert!(filter.matches("file-darwin-x86_64"));
        assert!(filter.matches("file-windows-x86_64"));
        assert!(!filter.matches("file-linux-arm64"));
    }

    #[test]
    fn test_glob_question_mark() {
        let filter = Filter {
            regexes: vec![],
            globs: vec!["file-?.tar.gz".to_string()],
            include: vec![],
            exclude: vec![],
            case_sensitive: true,
        };

        assert!(filter.matches("file-1.tar.gz"));
        assert!(filter.matches("file-a.tar.gz"));
        assert!(!filter.matches("file-10.tar.gz"));
        assert!(!filter.matches("file-.tar.gz"));
    }
}

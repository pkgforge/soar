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
    pub fn matches(&self, name: &str) -> bool {
        let matches_regex = self.regexes.iter().all(|r| r.is_match(name));
        let matches_glob = self.globs.iter().any(|g| glob_match(g, name));
        let matches_include = self.matches_keywords(name, &self.include, true);
        let matches_exclude = !self.matches_keywords(name, &self.exclude, false);

        matches_regex && matches_glob && matches_include && matches_exclude
    }

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

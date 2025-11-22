pub fn default_install_patterns() -> Vec<String> {
    ["!*.log", "!SBUILD", "!*.json", "!*.version"]
        .into_iter()
        .map(String::from)
        .collect::<Vec<String>>()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_install_patterns() {
        let patterns = default_install_patterns();

        assert_eq!(patterns.len(), 4);
        assert!(patterns.contains(&"!*.log".to_string()));
        assert!(patterns.contains(&"!SBUILD".to_string()));
        assert!(patterns.contains(&"!*.json".to_string()));
        assert!(patterns.contains(&"!*.version".to_string()));
    }
}

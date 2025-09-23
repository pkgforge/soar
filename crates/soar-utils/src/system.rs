/// Retrieves the platform string in the format `ARCH-Os`.
///
/// This function combines the architecture (e.g., `x86_64`) and the operating
/// system (e.g., `Linux`) into a single string to identify the platform.
pub fn platform() -> String {
    format!(
        "{}-{}{}",
        std::env::consts::ARCH,
        &std::env::consts::OS[..1].to_uppercase(),
        &std::env::consts::OS[1..]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform() {
        #[cfg(target_arch = "x86_64")]
        #[cfg(target_os = "linux")]
        assert_eq!(platform(), "x86_64-Linux");

        #[cfg(target_arch = "aarch64")]
        #[cfg(target_os = "linux")]
        assert_eq!(platform(), "aarch64-Linux");
    }
}

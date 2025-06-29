#[derive(Default)]
pub struct DefaultRepositoryInfo {
    pub name: &'static str,
    pub url_template: &'static str,
    pub pubkey: Option<&'static str>,
    pub desktop_integration: Option<bool>,
    pub enabled: Option<bool>,
    pub signature_verification: Option<bool>,
    pub sync_interval: Option<&'static str>,
    pub platforms: Vec<&'static str>,
    pub is_core: bool,
}

/// Returns a list of default repositories with their rules and supported platforms.
pub fn get_platform_repositories() -> Vec<DefaultRepositoryInfo> {
    vec![
        DefaultRepositoryInfo {
            name: "bincache",
            url_template: "https://meta.pkgforge.dev/bincache/{}.sdb.zstd",
            pubkey: Some("https://meta.pkgforge.dev/bincache/minisign.pub"),
            desktop_integration: Some(false),
            enabled: Some(true),
            signature_verification: Some(true),
            sync_interval: Some("3h"),
            platforms: vec!["aarch64-Linux", "riscv64-Linux", "x86_64-Linux"],
            is_core: true,
            ..DefaultRepositoryInfo::default()
        },
        DefaultRepositoryInfo {
            name: "pkgcache",
            url_template: "https://meta.pkgforge.dev/pkgcache/{}.sdb.zstd",
            pubkey: Some("https://meta.pkgforge.dev/pkgcache/minisign.pub"),
            desktop_integration: Some(true),
            platforms: vec!["aarch64-Linux", "riscv64-Linux", "x86_64-Linux"],
            is_core: true,
            ..DefaultRepositoryInfo::default()
        },
        DefaultRepositoryInfo {
            name: "pkgforge-cargo",
            url_template: "https://meta.pkgforge.dev/external/pkgforge-cargo/{}.sdb.zstd",
            desktop_integration: Some(false),
            platforms: vec![
                "aarch64-Linux",
                "loongarch64-Linux",
                "riscv64-Linux",
                "x86_64-Linux",
            ],
            is_core: true,
            ..DefaultRepositoryInfo::default()
        },
        DefaultRepositoryInfo {
            name: "pkgforge-go",
            url_template: "https://meta.pkgforge.dev/external/pkgforge-go/{}.sdb.zstd",
            desktop_integration: Some(false),
            platforms: vec![
                "aarch64-Linux",
                "loongarch64-Linux",
                "riscv64-Linux",
                "x86_64-Linux",
            ],
            is_core: true,
            ..DefaultRepositoryInfo::default()
        },
        DefaultRepositoryInfo {
            name: "ivan-hc-am",
            url_template: "https://meta.pkgforge.dev/external/am/{}.sdb.zstd",
            desktop_integration: Some(true),
            platforms: vec!["x86_64-Linux"],
            is_core: false,
            ..DefaultRepositoryInfo::default()
        },
        DefaultRepositoryInfo {
            name: "appimage-github-io",
            url_template: "https://meta.pkgforge.dev/external/appimage.github.io/{}.sdb.zstd",
            desktop_integration: Some(true),
            platforms: vec!["aarch64-Linux", "x86_64-Linux"],
            is_core: false,
            ..DefaultRepositoryInfo::default()
        },
    ]
}

use std::process::Command;

fn main() {
    if std::env::var("SOAR_NIGHTLY").is_ok() {
        let commit_sha = Command::new("git")
            .arg("rev-parse")
            .arg("--short")
            .arg("HEAD")
            .output()
            .expect("Failed to get git commit SHA")
            .stdout;

        let commit_sha = String::from_utf8(commit_sha)
            .expect("Invalid UTF-8 output")
            .trim()
            .to_string();

        println!("cargo:rerun-if-changed=build.rs");
        println!("cargo:rustc-env=CARGO_PKG_VERSION=nightly-{}", commit_sha);
    }
}

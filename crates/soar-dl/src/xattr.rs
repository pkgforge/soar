use std::path::Path;

use crate::types::ResumeInfo;

const XATTR_RESUME_KEY: &str = "user.soar.resume";

pub fn read_resume<P: AsRef<Path>>(path: P) -> Option<ResumeInfo> {
    xattr::get(path, XATTR_RESUME_KEY)
        .ok()
        .flatten()
        .and_then(|v| serde_json::from_slice(&v).ok())
}

pub fn write_resume<P: AsRef<Path>>(path: P, info: &ResumeInfo) -> std::io::Result<()> {
    xattr::set(path, XATTR_RESUME_KEY, &serde_json::to_vec(info)?)
}

pub fn remove_resume<P: AsRef<Path>>(path: P) -> std::io::Result<()> {
    xattr::remove(path, XATTR_RESUME_KEY)
}

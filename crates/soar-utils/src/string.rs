use std::sync::LazyLock;

use regex::Regex;

static ENCODED_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(%[A-Fa-f0-9]{2})+").expect("unable to compile encoded url regex")
});

/// Decode percent-encoded octet runs into their UTF-8 characters.
///
/// Replaces contiguous percent-encoded bytes (for example `%2F%41`) with the UTF‑8 string those bytes represent. Invalid hex pairs are skipped and any invalid UTF‑8 is replaced with the Unicode replacement character.
///
/// # Examples
///
/// ```
/// let s = "path%2Fto%2Ffile%20name%20with%20%C3%A9";
/// assert_eq!(decode_uri(s), "path/to/file name with é");
/// ```
pub fn decode_uri(s: &str) -> String {
    ENCODED_RE
        .replace_all(s, |caps: &regex::Captures| {
            let seq = &caps[0];
            let bytes: Vec<u8> = seq
                .as_bytes()
                .chunks(3)
                .filter_map(|chunk| {
                    if chunk.len() == 3 && chunk[0] == b'%' {
                        u8::from_str_radix(std::str::from_utf8(&chunk[1..3]).ok()?, 16).ok()
                    } else {
                        None
                    }
                })
                .collect();
            String::from_utf8_lossy(&bytes).into_owned()
        })
        .into_owned()
}
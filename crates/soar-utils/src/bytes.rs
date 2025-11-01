use crate::error::{BytesError, BytesResult};

/// Formats a number of bytes into a human-readable string.
///
/// This method converts a byte count into a string with appropriate units (B, KiB, MiB, etc.)
/// and a specified level of precision.
///
/// # Arguments
///
/// * `bytes` - The number of bytes to format
/// * `precision` - The number of decimal places to display
///
/// # Returns
///
/// A human-readable string representation of the byte count.
///
/// # Example
///
/// ```
/// use soar_utils::bytes::format_bytes;
///
/// let bytes = 1024_u64.pow(2);
/// let formatted = format_bytes(bytes, 2);
///
/// assert_eq!(formatted, "1.00 MiB");
/// ```
pub fn format_bytes(bytes: u64, precision: usize) -> String {
    let unit = 1024.0;
    let sizes = ["B", "KiB", "MiB", "GiB", "TiB", "PiB", "EiB"];

    let idx = (bytes as f64).log(unit).floor() as usize;
    let idx = idx.min(sizes.len() - 1);

    format!(
        "{:.*} {}",
        precision,
        bytes as f64 / unit.powi(idx.try_into().unwrap()),
        sizes[idx]
    )
}

/// Parses a human-readable byte string into a number of bytes.
///
/// This method converts a string with units (e.g., "1.00 MiB", "1KB") into a `u64` byte count.
/// It supports both binary (KiB, MiB) and decimal (KB, MB) prefixes.
///
/// # Arguments
///
/// * `s` - The string to parse
///
/// # Returns
///
/// Returns the number of bytes as a `u64`, or a [`BytesError`] if the string is invalid.
///
/// # Errors
///
/// * [`BytesError::ParseFailed`] if the string has an invalid format or suffix.
///
/// # Example
///
/// ```
/// use soar_utils::bytes::parse_bytes;
///
/// let bytes = parse_bytes("1.00 MiB").unwrap();
///
/// assert_eq!(bytes, 1024_u64.pow(2));
/// ```
pub fn parse_bytes(s: &str) -> BytesResult<u64> {
    let mut size = s.trim().to_uppercase();

    // If it's a number, just return it
    if let Ok(v) = size.parse::<u64>() {
        return Ok(v);
    };

    let prefixes = ["", "K", "M", "G", "T", "P", "E"];

    let base: f64 = if size.ends_with("IB") {
        size.truncate(size.len() - 2);
        1024.0
    } else if size.ends_with("B") {
        size.truncate(size.len() - 1);
        1000.0
    } else {
        return Err(BytesError::ParseFailed {
            input: s.to_string(),
            reason: "Invalid suffix".to_string(),
        });
    };

    prefixes
        .iter()
        .enumerate()
        .rev()
        .find_map(|(i, p)| {
            size.strip_suffix(p).and_then(|num| {
                num.trim()
                    .parse::<f64>()
                    .ok()
                    .map(|n| n * base.powi(i.try_into().unwrap()))
                    .map(|n| n.round() as u64)
            })
        })
        .ok_or_else(|| {
            BytesError::ParseFailed {
                input: s.to_string(),
                reason: "Unrecognized size format".into(),
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes_with_precisions() {
        assert_eq!(format_bytes(1111, 0), "1 KiB");

        assert_eq!(format_bytes(0, 0), "0 B");
        assert_eq!(format_bytes(0, 3), "0.000 B");
        assert_eq!(format_bytes(1023, 0), "1023 B");
        assert_eq!(format_bytes(1023, 2), "1023.00 B");

        assert_eq!(format_bytes(1024, 0), "1 KiB");
        assert_eq!(format_bytes(1024, 1), "1.0 KiB");
        assert_eq!(format_bytes(1536, 2), "1.50 KiB");
        assert_eq!(format_bytes(2047, 3), "1.999 KiB");
        assert_eq!(format_bytes(2048, 4), "2.0000 KiB");

        assert_eq!(format_bytes(1024_u64.pow(2), 0), "1 MiB");
        assert_eq!(format_bytes(3 * 1024_u64.pow(2) / 2, 2), "1.50 MiB");
        assert_eq!(format_bytes(2 * 1024_u64.pow(2) - 1, 3), "2.000 MiB");

        assert_eq!(format_bytes(1024_u64.pow(3), 2), "1.00 GiB");
        assert_eq!(format_bytes(5 * 1024_u64.pow(3) / 2, 1), "2.5 GiB");

        assert_eq!(format_bytes(1024_u64.pow(4), 3), "1.000 TiB");
        assert_eq!(format_bytes(3 * 1024_u64.pow(4) / 2, 2), "1.50 TiB");

        assert_eq!(format_bytes(1024_u64.pow(5), 0), "1 PiB");
        assert_eq!(
            format_bytes(1024_u64.pow(5) + 512 * 1024_u64.pow(4), 2),
            "1.50 PiB"
        );

        assert_eq!(format_bytes(1024_u64.pow(6), 1), "1.0 EiB");
        assert_eq!(
            format_bytes(1024_u64.pow(6) + 512 * 1024_u64.pow(5), 3),
            "1.500 EiB"
        );
    }

    #[test]
    fn test_parse_bytes() {
        assert_eq!(parse_bytes("111").unwrap(), 111);

        assert_eq!(parse_bytes("42").unwrap(), 42);
        assert_eq!(parse_bytes(" 120 ").unwrap(), 120);

        assert_eq!(parse_bytes("0B").unwrap(), 0);
        assert_eq!(parse_bytes("1B").unwrap(), 1);
        assert_eq!(parse_bytes("1023B").unwrap(), 1023);

        assert_eq!(parse_bytes("1KiB").unwrap(), 1024);
        assert_eq!(parse_bytes("1.50KiB").unwrap(), 3 * 1024 / 2);
        assert_eq!(parse_bytes("1KB").unwrap(), 1000);
        assert_eq!(parse_bytes("1.50KB").unwrap(), 3 * 1000 / 2);

        assert_eq!(parse_bytes("1MiB").unwrap(), 1024_u64.pow(2));
        assert_eq!(parse_bytes("1.50MiB").unwrap(), 3 * 1024_u64.pow(2) / 2);
        assert_eq!(parse_bytes("1MB").unwrap(), 1000_u64.pow(2));
        assert_eq!(parse_bytes("1.50MB").unwrap(), 3 * 1000_u64.pow(2) / 2);

        assert_eq!(parse_bytes("1GiB").unwrap(), 1024_u64.pow(3));
        assert_eq!(parse_bytes("1.50GiB").unwrap(), 3 * 1024_u64.pow(3) / 2);
        assert_eq!(parse_bytes("1GB").unwrap(), 1000_u64.pow(3));
        assert_eq!(parse_bytes("1.50GB").unwrap(), 3 * 1000_u64.pow(3) / 2);

        assert_eq!(parse_bytes("1TiB").unwrap(), 1024_u64.pow(4));
        assert_eq!(parse_bytes("1.50TiB").unwrap(), 3 * 1024_u64.pow(4) / 2);
        assert_eq!(parse_bytes("1TB").unwrap(), 1000_u64.pow(4));
        assert_eq!(parse_bytes("1.50TB").unwrap(), 3 * 1000_u64.pow(4) / 2);

        assert_eq!(parse_bytes("1PiB").unwrap(), 1024_u64.pow(5));
        assert_eq!(parse_bytes("1.50PiB").unwrap(), 3 * 1024_u64.pow(5) / 2);
        assert_eq!(parse_bytes("1PB").unwrap(), 1000_u64.pow(5));
        assert_eq!(parse_bytes("1.50PB").unwrap(), 3 * 1000_u64.pow(5) / 2);

        assert_eq!(parse_bytes("1EiB").unwrap(), 1024_u64.pow(6));
        assert_eq!(parse_bytes("1.50EiB").unwrap(), 3 * 1024_u64.pow(6) / 2);
        assert_eq!(parse_bytes("1EB").unwrap(), 1000_u64.pow(6));
        assert_eq!(parse_bytes("1.50EB").unwrap(), 3 * 1000_u64.pow(6) / 2);
    }

    #[test]
    fn test_fail_parse_bytes() {
        assert!(parse_bytes("1.xE").is_err());
        assert!(parse_bytes("1.xEB").is_err());
        assert!(parse_bytes("1.50FB").is_err());
        assert!(parse_bytes("1LB ").is_err());
        assert!(parse_bytes(" 1.50Li").is_err());
        assert!(parse_bytes(" MiB ").is_err());
        assert!(parse_bytes("MB").is_err());
    }
}

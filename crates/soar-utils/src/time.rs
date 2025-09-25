/// Parses a duration string into a number of milliseconds.
///
/// This function takes a string in the format `1d1h1m1s` and parses it into
/// a number of milliseconds. The string can contain any number of digits,
/// followed by any combination of the letters `s`, `m`, `h`, and `d` to
/// represent seconds, minutes, hours, and days, respectively.
///
/// # Arguments
/// * `input` - A string in the format `1d1h1m1s1`.
///
/// # Returns
/// A number of milliseconds, or `None` if the input string is invalid.
/// If the integer overflows, the function returns `None`.
///
/// # Examples
///
/// ```
/// use soar_utils::time::parse_duration;
///
/// let duration = parse_duration("1d1h1m1s");
/// println!("Duration: {}", duration.unwrap());
/// ```
pub fn parse_duration(input: &str) -> Option<u128> {
    let mut total: u128 = 0;
    let mut chars = input.chars().peekable();

    while chars.peek().is_some() {
        let mut number_str = String::new();
        while let Some(c) = chars.peek() {
            if c.is_ascii_digit() {
                number_str.push(chars.next()?);
            } else {
                break;
            }
        }

        if number_str.is_empty() {
            return None;
        }

        let number: u128 = number_str.parse().ok()?;
        let multiplier = match chars.next()? {
            's' => 1000,
            'm' => 60 * 1000,
            'h' => 60 * 60 * 1000,
            'd' => 24 * 60 * 60 * 1000,
            _ => return None,
        };

        total += number * multiplier;
    }

    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("1s"), Some(1000));
        assert_eq!(parse_duration("1m"), Some(60 * 1000));
        assert_eq!(parse_duration("1h"), Some(60 * 60 * 1000));
        assert_eq!(parse_duration("1d"), Some(24 * 60 * 60 * 1000));
        assert_eq!(
            parse_duration("1d1h"),
            Some(24 * 60 * 60 * 1000 + 60 * 60 * 1000)
        );
        assert_eq!(
            parse_duration("1d1h1m"),
            Some(24 * 60 * 60 * 1000 + 60 * 60 * 1000 + 60 * 1000)
        );
        assert_eq!(
            parse_duration("1d1h1m1s"),
            Some(24 * 60 * 60 * 1000 + 60 * 60 * 1000 + 60 * 1000 + 1000)
        );
        assert_eq!(parse_duration("1d1h1m1s1"), None);
        assert_eq!(parse_duration("1d1h1m1s1a"), None);
        assert_eq!(parse_duration("fail"), None);
        assert_eq!(parse_duration(""), Some(0));
    }

    #[test]
    fn test_integer_overflow() {
        assert_eq!(
            parse_duration("340282366920938463463374607431768211456"),
            None
        );
    }
}

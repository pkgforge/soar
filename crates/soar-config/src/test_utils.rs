#[cfg(test)]
pub fn with_env<F>(vars: Vec<(&str, &str)>, f: F)
where
    F: FnOnce(),
{
    let old_vars: Vec<_> = vars
        .iter()
        .map(|(k, _)| (*k, std::env::var(k).ok()))
        .collect();

    for (key, value) in &vars {
        std::env::set_var(key, value);
    }

    f();

    for (key, old_value) in old_vars {
        match old_value {
            Some(v) => std::env::set_var(key, v),
            None => std::env::remove_var(key),
        }
    }
}

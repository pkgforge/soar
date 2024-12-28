use std::io::Write;

use soar_core::SoarResult;

pub fn interactive_ask(ques: &str) -> SoarResult<String> {
    print!("{}", ques);

    std::io::stdout().flush()?;

    let mut response = String::new();
    std::io::stdin().read_line(&mut response)?;

    Ok(response.trim().to_owned())
}

pub fn normalize(call: &str) -> Option<String> {
    let call = call.trim().to_ascii_uppercase();
    let valid = call.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'/')
        && call.bytes().any(|b| b.is_ascii_digit())
        && call.bytes().any(|b| b.is_ascii_alphabetic());
    valid.then_some(call)
}

pub fn base(call: &str) -> String {
    call.trim()
        .split('/')
        .reduce(|a, b| if b.len() > a.len() { b } else { a })
        .unwrap_or_default()
        .to_ascii_uppercase()
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(" jl1his ", Some("JL1HIS"))]
    #[case("JL1HIS/1", Some("JL1HIS/1"))]
    #[case("", None)]
    #[case("JL1 HIS", None)]
    #[case("JLHIS", None)]
    #[case("1234", None)]
    #[case("JL1-HIS", None)]
    fn normalizes_callsigns(#[case] input: &str, #[case] expected: Option<&str>) {
        assert_eq!(normalize(input).as_deref(), expected);
    }

    #[rstest]
    #[case("JL1HIS", "JL1HIS")]
    #[case("jl1his", "JL1HIS")]
    #[case(" JL1HIS ", "JL1HIS")]
    #[case("JL1HIS/1", "JL1HIS")]
    #[case("JL1HIS/P", "JL1HIS")]
    #[case("JA1XYZ/QRP", "JA1XYZ")]
    #[case("KH6/W1AW", "W1AW")]
    #[case("3D2/JA1ABC", "JA1ABC")]
    #[case("KH6/JA1ABC/P", "JA1ABC")]
    #[case("VP2E/W1AW", "VP2E")]
    fn keeps_the_longest_segment(#[case] call: &str, #[case] expected: &str) {
        assert_eq!(base(call), expected);
    }
}

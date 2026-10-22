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

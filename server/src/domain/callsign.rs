pub use jl1his_query::call_base as base;

pub fn normalize(call: &str) -> Option<String> {
    let call = call.trim().to_ascii_uppercase();
    let valid = call.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'/')
        && call.bytes().any(|b| b.is_ascii_digit())
        && call.bytes().any(|b| b.is_ascii_alphabetic());
    valid.then_some(call)
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
}

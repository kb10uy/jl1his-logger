use thiserror::Error;
use url::{Host, Url};

#[derive(Debug, Error, PartialEq, Eq)]
pub enum OriginError {
    #[error("not a URL: {0}")]
    NotUrl(String),
    #[error("the URL must start with https://")]
    NotHttps,
}

pub fn parse_origin(input: &str) -> Result<Url, OriginError> {
    let url = Url::parse(input.trim()).map_err(|e| OriginError::NotUrl(e.to_string()))?;
    let local = matches!(url.host(), Some(Host::Domain("localhost")))
        || url.host().is_some_and(|host| match host {
            Host::Ipv4(ip) => ip.is_loopback(),
            Host::Ipv6(ip) => ip.is_loopback(),
            Host::Domain(_) => false,
        });
    match url.scheme() {
        "https" => {}
        "http" if local => {}
        "http" => return Err(OriginError::NotHttps),
        _ => return Err(OriginError::NotUrl(input.trim().to_owned())),
    }
    let origin = url.origin().ascii_serialization();
    Url::parse(&origin).map_err(|e| OriginError::NotUrl(e.to_string()))
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("https://log.example.com", "https://log.example.com/")]
    #[case(" https://log.example.com/ ", "https://log.example.com/")]
    #[case("https://log.example.com/logbooks/abc?q=call:JA1*#x", "https://log.example.com/")]
    #[case("https://log.example.com:8443", "https://log.example.com:8443/")]
    #[case("http://localhost:8080", "http://localhost:8080/")]
    #[case("http://127.0.0.1:8080", "http://127.0.0.1:8080/")]
    #[case("http://[::1]:8080", "http://[::1]:8080/")]
    fn keeps_the_origin(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(parse_origin(input).unwrap().as_str(), expected);
    }

    #[rstest]
    #[case("http://log.example.com", OriginError::NotHttps)]
    #[case("http://192.168.1.10", OriginError::NotHttps)]
    fn refuses_plain_http_off_this_machine(#[case] input: &str, #[case] expected: OriginError) {
        assert_eq!(parse_origin(input), Err(expected));
    }

    #[rstest]
    #[case("")]
    #[case("log.example.com")]
    #[case("ftp://log.example.com")]
    fn refuses_what_is_not_a_web_url(#[case] input: &str) {
        assert!(matches!(parse_origin(input), Err(OriginError::NotUrl(_))));
    }
}

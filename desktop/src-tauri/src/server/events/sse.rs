use std::mem;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct SseEvent {
    pub event: String,
    pub id: Option<String>,
    pub data: String,
}

#[derive(Debug, Default)]
pub struct SseParser {
    buffer: Vec<u8>,
    current: SseEvent,
    has_data: bool,
}

impl SseParser {
    pub fn feed(&mut self, bytes: &[u8]) -> Vec<SseEvent> {
        self.buffer.extend_from_slice(bytes);
        let mut events = Vec::new();
        while let Some(end) = self.buffer.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = self.buffer.drain(..=end).collect();
            let line = String::from_utf8_lossy(&line);
            let line = line.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                let event = mem::take(&mut self.current);
                if mem::take(&mut self.has_data) {
                    events.push(event);
                }
                continue;
            }
            if line.starts_with(':') {
                continue;
            }
            let (field, value) = match line.split_once(':') {
                Some((field, value)) => (field, value.strip_prefix(' ').unwrap_or(value)),
                None => (line, ""),
            };
            match field {
                "event" => self.current.event = value.to_owned(),
                "id" => self.current.id = Some(value.to_owned()),
                "data" => {
                    if self.has_data {
                        self.current.data.push('\n');
                    }
                    self.current.data.push_str(value);
                    self.has_data = true;
                }
                _ => {}
            }
        }
        events
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(event: &str, id: Option<&str>, data: &str) -> SseEvent {
        SseEvent {
            event: event.to_owned(),
            id: id.map(str::to_owned),
            data: data.to_owned(),
        }
    }

    #[test]
    fn reads_events_split_across_chunks() {
        let mut parser = SseParser::default();
        assert!(parser.feed(b"event: hello\ndata: {\"a\"").is_empty());
        assert_eq!(
            parser.feed(b":1}\n\nevent: qso\r\nid: 41\r\ndata: {}\r\n\r\n"),
            vec![event("hello", None, "{\"a\":1}"), event("qso", Some("41"), "{}")]
        );
    }

    #[test]
    fn skips_comments_and_events_without_data() {
        let mut parser = SseParser::default();
        assert!(parser.feed(b": keep-alive\n\nevent: ping\n\n").is_empty());
        assert_eq!(parser.feed(b"data: x\n\n"), vec![event("", None, "x")]);
    }

    #[test]
    fn joins_data_lines() {
        let mut parser = SseParser::default();
        assert_eq!(parser.feed(b"data: a\ndata:b\n\n"), vec![event("", None, "a\nb")]);
    }

    #[test]
    fn keeps_a_multibyte_character_split_between_chunks() {
        let mut parser = SseParser::default();
        let text = "data: 東京\n\n".as_bytes();
        assert!(parser.feed(&text[..8]).is_empty());
        assert_eq!(parser.feed(&text[8..]), vec![event("", None, "東京")]);
    }
}

use crate::listen::datagram::{EntryMessage, Fields};

const FORM_FIELDS: [&str; 7] = [
    "CALL",
    "RST_SENT",
    "RST_RCVD",
    "SRX_STRING",
    "STX_STRING",
    "NAME",
    "QTH",
];
const MAX_HELD_FIELDS: usize = 64;

#[derive(Debug, Default)]
pub struct EntryForm {
    attached: bool,
    held: Fields,
    form: Option<Fields>,
}

impl EntryForm {
    pub fn receive(&mut self, message: EntryMessage) -> Option<EntryMessage> {
        if self.attached {
            return Some(message);
        }
        match message {
            EntryMessage::Set { fields } => {
                for (name, value) in fields {
                    let name = name.trim().to_ascii_uppercase();
                    if self.held.len() < MAX_HELD_FIELDS || self.held.contains_key(&name) {
                        self.held.insert(name, value);
                    }
                }
            }
            EntryMessage::Clear => self.held.clear(),
            EntryMessage::Log => {}
        }
        None
    }

    pub fn attach(&mut self) -> Fields {
        self.attached = true;
        std::mem::take(&mut self.held)
    }

    pub fn detach(&mut self) {
        self.attached = false;
    }

    pub fn update(&mut self, fields: &Fields) -> Option<&Fields> {
        let form: Fields = FORM_FIELDS
            .iter()
            .map(|&name| (name.to_owned(), fields.get(name).cloned().unwrap_or_default()))
            .collect();
        if self.form.as_ref() == Some(&form) {
            return None;
        }
        self.form = Some(form);
        self.form.as_ref()
    }

    pub fn form(&self) -> Option<&Fields> {
        self.form.as_ref()
    }

    pub fn forget(&mut self) {
        self.form = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fields(pairs: &[(&str, &str)]) -> Fields {
        pairs.iter().map(|(n, v)| (n.to_string(), v.to_string())).collect()
    }

    fn set(pairs: &[(&str, &str)]) -> EntryMessage {
        EntryMessage::Set { fields: fields(pairs) }
    }

    #[test]
    fn passes_messages_through_while_attached() {
        let mut entry = EntryForm::default();
        assert!(entry.attach().is_empty());
        assert_eq!(
            entry.receive(set(&[("CALL", "JA1ABC")])),
            Some(set(&[("CALL", "JA1ABC")]))
        );
        assert_eq!(entry.receive(EntryMessage::Log), Some(EntryMessage::Log));
        assert!(entry.attach().is_empty());
    }

    #[test]
    fn holds_the_latest_value_of_each_field_until_attached() {
        let mut entry = EntryForm::default();
        assert_eq!(entry.receive(set(&[("CALL", "JA1ABC"), ("NAME", "Taro")])), None);
        assert_eq!(entry.receive(EntryMessage::Log), None);
        assert_eq!(entry.receive(set(&[(" call ", "JA2XYZ"), ("QTH", "")])), None);
        assert_eq!(
            entry.attach(),
            fields(&[("CALL", "JA2XYZ"), ("NAME", "Taro"), ("QTH", "")])
        );
        entry.detach();
        assert!(entry.attach().is_empty());
    }

    #[test]
    fn a_clear_drops_what_is_held() {
        let mut entry = EntryForm::default();
        entry.receive(set(&[("CALL", "JA1ABC")]));
        assert_eq!(entry.receive(EntryMessage::Clear), None);
        entry.receive(set(&[("NAME", "Taro")]));
        assert_eq!(entry.attach(), fields(&[("NAME", "Taro")]));
    }

    #[test]
    fn holds_a_limited_number_of_fields() {
        let mut entry = EntryForm::default();
        let many: Vec<(String, String)> = (0..MAX_HELD_FIELDS + 1)
            .map(|n| (format!("APP_TEST_{n}"), n.to_string()))
            .collect();
        entry.receive(EntryMessage::Set {
            fields: many.into_iter().collect(),
        });
        entry.receive(set(&[("APP_TEST_0", "again"), ("CALL", "JA1ABC")]));
        let held = entry.attach();
        assert_eq!(held.len(), MAX_HELD_FIELDS);
        assert_eq!(held["APP_TEST_0"], "again");
        assert!(!held.contains_key("CALL"));
    }

    #[test]
    fn keeps_every_form_field_and_only_those() {
        let mut entry = EntryForm::default();
        assert_eq!(entry.form(), None);
        let given = fields(&[("CALL", "JA1ABC"), ("SRX_STRING", "012"), ("FREQ", "7.012")]);
        assert_eq!(
            entry.update(&given),
            Some(&fields(&[
                ("CALL", "JA1ABC"),
                ("NAME", ""),
                ("QTH", ""),
                ("RST_RCVD", ""),
                ("RST_SENT", ""),
                ("SRX_STRING", "012"),
                ("STX_STRING", ""),
            ]))
        );
        assert_eq!(entry.update(&given), None);
        assert!(entry.update(&fields(&[("CALL", "JA1ABC")])).is_some());
        entry.forget();
        assert_eq!(entry.form(), None);
    }
}

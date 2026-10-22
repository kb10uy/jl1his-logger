use std::{fmt, str::FromStr};

use uuid::{Builder, Uuid, Variant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QsoId(Uuid);

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{0:?} is not a UUID v7")]
pub struct InvalidQsoId(String);

impl QsoId {
    pub fn generate() -> QsoId {
        QsoId(Uuid::now_v7())
    }

    pub fn with_time(unix_millis: u64, rest: &[u8; 10]) -> QsoId {
        QsoId(Builder::from_unix_timestamp_millis(unix_millis, rest).into_uuid())
    }
}

impl FromStr for QsoId {
    type Err = InvalidQsoId;

    fn from_str(s: &str) -> Result<QsoId, InvalidQsoId> {
        match Uuid::parse_str(s) {
            Ok(id) if id.get_version_num() == 7 && id.get_variant() == Variant::RFC4122 => Ok(QsoId(id)),
            _ => Err(InvalidQsoId(s.to_owned())),
        }
    }
}

impl fmt::Display for QsoId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.hyphenated().fmt(f)
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case("019a0a4e-6f2b-7c3d-8e4f-0123456789ab", "019a0a4e-6f2b-7c3d-8e4f-0123456789ab")]
    #[case("019A0A4E-6F2B-7C3D-8E4F-0123456789AB", "019a0a4e-6f2b-7c3d-8e4f-0123456789ab")]
    #[case("019a0a4e6f2b7c3d8e4f0123456789ab", "019a0a4e-6f2b-7c3d-8e4f-0123456789ab")]
    fn reads_qso_ids_in_canonical_form(#[case] input: &str, #[case] canonical: &str) {
        assert_eq!(input.parse::<QsoId>().unwrap().to_string(), canonical);
    }

    #[rstest]
    #[case("6f1c2a3e-4b5d-4e6f-8a7b-0123456789ab")]
    #[case("019a0a4e-6f2b-7c3d-0e4f-0123456789ab")]
    #[case("00000000-0000-0000-0000-000000000000")]
    #[case("not-a-uuid")]
    fn refuses_ids_other_than_uuid_v7(#[case] input: &str) {
        assert!(input.parse::<QsoId>().is_err());
    }

    #[test]
    fn generates_uuid_v7() {
        let id = QsoId::generate();
        assert_eq!(id.to_string().parse::<QsoId>(), Ok(id));
    }

    #[test]
    fn builds_uuid_v7_from_a_time_and_given_bits() {
        let id = QsoId::with_time(1_792_641_906_000, &[0xff; 10]);
        assert_eq!(id.to_string(), "01a161c8-b550-7fff-bfff-ffffffffffff");
        assert_eq!(id.to_string().parse::<QsoId>(), Ok(id));
    }
}

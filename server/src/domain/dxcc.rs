#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DxccEntity {
    pub code: u32,
    pub name: &'static str,
    pub deleted: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Subdivision {
    pub dxcc: u32,
    pub code: &'static str,
    pub name: &'static str,
}

const fn entity(code: u32, name: &'static str, deleted: bool) -> DxccEntity {
    DxccEntity { code, name, deleted }
}

const fn subdivision(dxcc: u32, code: &'static str, name: &'static str) -> Subdivision {
    Subdivision { dxcc, code, name }
}

include!(concat!(env!("OUT_DIR"), "/adif/dxcc.rs"));

impl DxccEntity {
    pub fn of_code(code: u32) -> Option<&'static DxccEntity> {
        ENTITIES
            .binary_search_by_key(&code, |e| e.code)
            .ok()
            .map(|i| &ENTITIES[i])
    }

    pub fn subdivisions(&self) -> &'static [Subdivision] {
        let start = SUBDIVISIONS.partition_point(|s| s.dxcc < self.code);
        let end = SUBDIVISIONS.partition_point(|s| s.dxcc <= self.code);
        &SUBDIVISIONS[start..end]
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[rstest]
    #[case(0, Some(("None (the contacted station is known to not be within a DXCC entity)", false)))]
    #[case(2, Some(("ABU AIL IS.", true)))]
    #[case(339, Some(("JAPAN", false)))]
    #[case(1000, None)]
    fn finds_entities_by_code(#[case] code: u32, #[case] expected: Option<(&str, bool)>) {
        assert_eq!(DxccEntity::of_code(code).map(|e| (e.name, e.deleted)), expected);
    }

    #[test]
    fn lists_the_current_subdivisions_of_an_entity_by_code() {
        let japan = DxccEntity::of_code(339).unwrap().subdivisions();
        assert_eq!(japan.len(), 47);
        assert_eq!(japan[0], subdivision(339, "01", "Hokkaido"));
        assert_eq!(japan[9], subdivision(339, "10", "Tokyo"));
        assert!(
            DxccEntity::of_code(1)
                .unwrap()
                .subdivisions()
                .iter()
                .any(|s| s.code == "QC")
        );
        assert!(DxccEntity::of_code(0).unwrap().subdivisions().is_empty());

        let mexico = DxccEntity::of_code(50).unwrap().subdivisions();
        assert!(mexico.iter().any(|s| s.code == "CMX"));
        assert!(
            !mexico.iter().any(|s| s.code == "DF"),
            "import-only codes are not choices"
        );
        let russia = DxccEntity::of_code(15).unwrap().subdivisions();
        assert!(!russia.iter().any(|s| s.code == "UO"), "deleted codes are not choices");
    }

    #[test]
    fn keeps_both_tables_ordered_for_searching() {
        assert!(ENTITIES.windows(2).all(|w| w[0].code < w[1].code));
        assert!(
            SUBDIVISIONS
                .windows(2)
                .all(|w| (w[0].dxcc, w[0].code) < (w[1].dxcc, w[1].code))
        );
        assert!(SUBDIVISIONS.iter().all(|s| DxccEntity::of_code(s.dxcc).is_some()));
    }
}

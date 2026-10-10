use std::sync::Arc;

use uuid::Uuid;
use zvs_contract::StreamId;

pub type IdGenerator = Arc<dyn Fn() -> Uuid + Send + Sync>;

pub fn new_id() -> Uuid {
    Uuid::now_v7()
}

pub fn new_stream_id() -> StreamId {
    StreamId::new(new_id())
}

pub fn system_ids() -> IdGenerator {
    Arc::new(new_id)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn entity_ids_are_unique_ordered_uuid_v7_values() {
        let ids: Vec<String> = (0..1000)
            .map(|_| new_id().hyphenated().to_string())
            .collect();
        assert_eq!(ids.iter().collect::<HashSet<_>>().len(), ids.len());
        let mut sorted = ids.clone();
        sorted.sort();
        assert_eq!(ids, sorted);
        for id in &ids {
            let bytes = id.as_bytes();
            assert_eq!(bytes.len(), 36);
            assert_eq!(bytes[14], b'7', "{id}");
            assert!(matches!(bytes[19], b'8' | b'9' | b'a' | b'b'), "{id}");
            assert!(
                id.chars()
                    .all(|c| c == '-' || c.is_ascii_digit() || ('a'..='f').contains(&c))
            );
        }
    }

    #[test]
    fn a_stream_id_survives_the_contract_parser() {
        let id = new_stream_id();
        assert_eq!(StreamId::parse(&id.to_string()), Some(id));
    }
}

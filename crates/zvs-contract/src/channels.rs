include!(concat!(env!("OUT_DIR"), "/channels.rs"));

pub const CHANNELS_JSON: &str = include_str!("../../../packages/shared/contract.channels.json");

pub fn is_channel(name: &str) -> bool {
    CHANNELS.binary_search(&name).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_list_parses_and_matches_the_constant() {
        let parsed: Vec<String> = serde_json::from_str(CHANNELS_JSON).expect("channel list parses");
        assert_eq!(parsed, CHANNELS);
    }

    #[test]
    fn the_list_is_sorted_and_unique() {
        assert!(!CHANNELS.is_empty());
        assert!(CHANNELS.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn channels_follow_the_domain_verb_shape() {
        for channel in CHANNELS {
            let (domain, verb) = channel.split_once('.').expect("domain.verb");
            assert!(!domain.is_empty() && !verb.is_empty(), "{channel}");
        }
    }

    #[test]
    fn lookup_finds_known_channels_only() {
        assert!(is_channel("system.ping"));
        assert!(!is_channel("system.nope"));
    }
}

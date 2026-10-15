use std::sync::LazyLock;

use regex::Regex;
use zvs_contract::validate::{js_slice, js_trim};

pub const FOLLOW_UP_MAX_WORDS: usize = 8;
pub const FOLLOW_UP_CONTEXT_CHARS: usize = 2000;

const OPENERS: &[&str] = &[
    "а",
    "и",
    "но",
    "или",
    "также",
    "тоже",
    "ещё",
    "еще",
    "тогда",
    "значит",
    "это",
    "этот",
    "эта",
    "эти",
    "этого",
    "этой",
    "этом",
    "этих",
    "тот",
    "та",
    "то",
    "те",
    "там",
    "тут",
    "он",
    "она",
    "оно",
    "они",
    "его",
    "её",
    "ее",
    "их",
    "ему",
    "ей",
    "им",
    "нём",
    "нем",
    "ней",
    "них",
    "такой",
    "такая",
    "такое",
    "такие",
    "and",
    "but",
    "or",
    "also",
    "so",
    "then",
    "it",
    "its",
    "this",
    "that",
    "these",
    "those",
    "they",
    "them",
    "their",
    "he",
    "she",
    "his",
    "her",
    "there",
    "what about",
];

const ACKNOWLEDGEMENTS: &[&str] = &[
    "спасибо",
    "спс",
    "благодарю",
    "большое",
    "огромное",
    "ок",
    "окей",
    "хорошо",
    "ясно",
    "понятно",
    "понял",
    "поняла",
    "отлично",
    "супер",
    "класс",
    "да",
    "нет",
    "привет",
    "пока",
    "thanks",
    "thank",
    "you",
    "thx",
    "ok",
    "okay",
    "great",
    "cool",
    "got",
    "it",
    "yes",
    "no",
    "hi",
    "hello",
    "bye",
];

static WORD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"[\p{L}\p{N}]+(?:[-'’][\p{L}\p{N}]+)*").expect("the word pattern compiles")
});

fn words(text: &str) -> Vec<String> {
    let lowered = text.to_lowercase();
    WORD.find_iter(&lowered)
        .map(|word| word.as_str().to_owned())
        .collect()
}

pub fn is_follow_up(text: &str) -> bool {
    let tokens = words(text);
    if tokens.is_empty()
        || tokens
            .iter()
            .all(|token| ACKNOWLEDGEMENTS.contains(&token.as_str()))
    {
        return false;
    }
    if tokens.len() < FOLLOW_UP_MAX_WORDS {
        return true;
    }
    let pair = format!("{} {}", tokens[0], tokens.get(1).map_or("", String::as_str));
    OPENERS.contains(&tokens[0].as_str()) || OPENERS.contains(&pair.as_str())
}

pub fn retrieval_query(current: &str, previous_user: Option<&str>) -> String {
    let text = js_trim(current);
    let previous = previous_user
        .map(|previous| js_trim(js_slice(js_trim(previous), FOLLOW_UP_CONTEXT_CHARS)))
        .unwrap_or("");
    if previous.is_empty() || !is_follow_up(text) {
        return text.to_owned();
    }
    format!("{previous}\n{text}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_or_pronoun_led_turns_are_searched_together_with_the_previous_question() {
        assert!(is_follow_up("А что во втором пункте?"));
        assert!(is_follow_up(
            "Это касается и договора подряда, заключённого между сторонами в 2020 году?"
        ));
        assert!(is_follow_up(
            "And what does it say about the termination of the agreement by either party?"
        ));
        assert!(!is_follow_up(
            "Какие сроки исковой давности установлены для споров по договору поставки товаров?"
        ));
        assert!(!is_follow_up("Спасибо!"));
        assert!(!is_follow_up("ok, thanks"));
        assert_eq!(
            retrieval_query(
                "А что во втором пункте?",
                Some("Какие условия договора поставки?")
            ),
            "Какие условия договора поставки?\nА что во втором пункте?"
        );
        assert_eq!(
            retrieval_query("А что во втором пункте?", None),
            "А что во втором пункте?"
        );
        assert_eq!(
            retrieval_query("спасибо", Some("Какие условия договора поставки?")),
            "спасибо"
        );
        assert_eq!(
            retrieval_query("Почему?", Some(&"x".repeat(5000))),
            format!("{}\nПочему?", "x".repeat(2000))
        );
    }

    #[test]
    fn a_two_word_opener_counts() {
        assert!(is_follow_up(
            "What about the clauses that cover termination by either of the parties?"
        ));
        assert!(!is_follow_up(""));
    }
}

use std::sync::LazyLock;

use regex::Regex;

pub const CYRILLIC_CHARS_PER_TOKEN: u64 = 3;
pub const LATIN_CHARS_PER_TOKEN: u64 = 4;

static CLASSES: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?s)(\p{Script=Cyrillic})|(\p{Script=Latin})|([\p{Nd}\p{Script=Han}\p{Script=Hiragana}\p{Script=Katakana}\p{Script=Hangul}\p{P}\p{S}])|([\s\x{feff}])|(.)",
    )
    .expect("a valid pattern")
});

pub fn estimate_tokens(text: &str) -> u64 {
    let (mut cyrillic, mut latin, mut single, mut other) = (0_u64, 0_u64, 0_u64, 0_u64);
    for captures in CLASSES.captures_iter(text) {
        if captures.get(1).is_some() {
            cyrillic += 1;
        } else if captures.get(2).is_some() {
            latin += 1;
        } else if captures.get(3).is_some() {
            single += 1;
        } else if captures.get(5).is_some() {
            other += 1;
        }
    }
    let scale = CYRILLIC_CHARS_PER_TOKEN * LATIN_CHARS_PER_TOKEN;
    let weighted = cyrillic * LATIN_CHARS_PER_TOKEN
        + latin * CYRILLIC_CHARS_PER_TOKEN
        + (single + other * 2) * scale;
    weighted.div_ceil(scale)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLES: &[(&str, u64, &str)] = &[
        (
            "russian prose",
            123,
            "Верховный суд Российской Федерации рассмотрел кассационную жалобу общества с ограниченной ответственностью на решение арбитражного суда и постановление апелляционного суда. Суд указал, что при разрешении спора нижестоящие инстанции неправильно применили нормы материального права, поскольку не учли условия договора поставки и фактические обстоятельства дела. Дело направлено на новое рассмотрение в суд первой инстанции.\n",
        ),
        (
            "english prose",
            80,
            "The Supreme Court reviewed the cassation appeal filed by the limited liability company against the decision of the arbitration court and the ruling of the appellate court. The court held that the lower courts had misapplied substantive law, because they did not take into account the terms of the supply agreement and the actual circumstances of the case. The case was remitted to the court of first instance for a new hearing.\n",
        ),
        (
            "russian question",
            42,
            "Спасибо! А что во втором пункте? Можешь объяснить подробнее, почему суд так решил и какие статьи Гражданского кодекса он применил?\n",
        ),
        (
            "russian with numbers",
            88,
            "Дело № А33-13638/2020: истец ООО «Ромашка» требовал взыскать 1 250 000 руб. по договору № 15/2019 от 01.03.2019 (см. ст. 309, 310 ГК РФ).\n",
        ),
    ];

    #[test]
    fn the_estimate_is_within_a_quarter_of_the_qwen_tokenizer() {
        for (name, tokens, text) in SAMPLES {
            let estimate = estimate_tokens(text);
            let error = estimate.abs_diff(*tokens) * 4;
            assert!(error <= *tokens, "{name}: {estimate} vs {tokens}");
        }
    }

    #[test]
    fn the_estimate_is_script_aware_and_never_cheaper_than_a_cjk_character_each() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("абв"), 1);
        assert_eq!(estimate_tokens("abcd"), 1);
        assert_eq!(estimate_tokens("最高法院"), 4);
        assert!(estimate_tokens("привет мир") * 4 < "привет мир".len() as u64);
        assert_eq!(estimate_tokens("\u{1F600}"), 1);
        assert_eq!(estimate_tokens("\u{0300}"), 2);
    }
}

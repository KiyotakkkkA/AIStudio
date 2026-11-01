use tiktoken_rs::{CoreBPE, o200k_base_singleton};

pub const TOKENIZER: &str = "o200k_base";

fn tokenizer() -> &'static CoreBPE {
    o200k_base_singleton()
}

pub fn estimate_tokens(text: &str) -> u64 {
    if text.is_empty() {
        return 0;
    }
    tokenizer().count_ordinary(text) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLES: &[(&str, u64, &str)] = &[
        (
            "russian prose",
            95,
            "Верховный суд Российской Федерации рассмотрел кассационную жалобу общества с ограниченной ответственностью на решение арбитражного суда и постановление апелляционного суда. Суд указал, что при разрешении спора нижестоящие инстанции неправильно применили нормы материального права, поскольку не учли условия договора поставки и фактические обстоятельства дела. Дело направлено на новое рассмотрение в суд первой инстанции.\n",
        ),
        (
            "english prose",
            80,
            "The Supreme Court reviewed the cassation appeal filed by the limited liability company against the decision of the arbitration court and the ruling of the appellate court. The court held that the lower courts had misapplied substantive law, because they did not take into account the terms of the supply agreement and the actual circumstances of the case. The case was remitted to the court of first instance for a new hearing.\n",
        ),
        (
            "russian question",
            32,
            "Спасибо! А что во втором пункте? Можешь объяснить подробнее, почему суд так решил и какие статьи Гражданского кодекса он применил?\n",
        ),
        (
            "russian with numbers",
            64,
            "Дело № А33-13638/2020: истец ООО «Ромашка» требовал взыскать 1 250 000 руб. по договору № 15/2019 от 01.03.2019 (см. ст. 309, 310 ГК РФ).\n",
        ),
    ];

    #[test]
    fn the_samples_count_exactly_as_o200k_base() {
        for (name, tokens, text) in SAMPLES {
            assert_eq!(estimate_tokens(text), *tokens, "{name}");
        }
    }

    #[test]
    fn the_count_is_the_tokenizer_count_and_ignores_special_token_syntax() {
        assert_eq!(estimate_tokens(""), 0);
        assert_eq!(estimate_tokens("hello"), 1);
        assert_eq!(
            estimate_tokens("hello world"),
            tokenizer().count_ordinary("hello world") as u64
        );
        assert!(estimate_tokens("<|endoftext|>") > 1);
    }

    #[test]
    fn a_long_text_costs_less_than_one_token_per_character() {
        let text = "Выручка компании за третий квартал выросла. ".repeat(50);
        assert!(estimate_tokens(&text) * 2 < text.chars().count() as u64);
    }
}

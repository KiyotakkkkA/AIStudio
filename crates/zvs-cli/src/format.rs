pub const GROUP_SEPARATOR: char = '\u{a0}';

pub fn number(value: u64) -> String {
    let digits = value.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (index, digit) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(GROUP_SEPARATOR);
        }
        out.push(digit);
    }
    out
}

pub fn plural(count: u64, one: &str, few: &str, many: &str) -> String {
    let tail = count % 100;
    let last = count % 10;
    let word = if (11..=14).contains(&tail) {
        many
    } else if last == 1 {
        one
    } else if (2..=4).contains(&last) {
        few
    } else {
        many
    };
    format!("{} {word}", number(count))
}

pub fn tokens(count: u64) -> String {
    plural(count, "токен", "токена", "токенов")
}

pub fn messages(count: u64) -> String {
    plural(count, "сообщение", "сообщения", "сообщений")
}

pub fn tools(count: u64) -> String {
    plural(count, "инструмент", "инструмента", "инструментов")
}

pub fn files(count: u64) -> String {
    plural(count, "файл", "файла", "файлов")
}

pub fn percent(part: u64, whole: u64) -> String {
    if whole == 0 {
        return "0\u{a0}%".to_owned();
    }
    let value = part.saturating_mul(100) / whole;
    format!("{}\u{a0}%", number(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_group_thousands_like_ru_ru() {
        assert_eq!(number(0), "0");
        assert_eq!(number(999), "999");
        assert_eq!(number(128_000), "128\u{a0}000");
        assert_eq!(number(1_234_567), "1\u{a0}234\u{a0}567");
    }

    #[test]
    fn plurals_follow_russian_rules() {
        assert_eq!(tokens(1), "1 токен");
        assert_eq!(tokens(3), "3 токена");
        assert_eq!(tokens(11), "11 токенов");
        assert_eq!(tokens(21), "21 токен");
        assert_eq!(messages(112), "112 сообщений");
        assert_eq!(percent(1, 4), "25\u{a0}%");
    }
}

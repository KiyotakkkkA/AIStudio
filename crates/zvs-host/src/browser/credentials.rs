use serde::Deserialize;

pub const CREDENTIAL_FIELD: &str =
    "Ввод в поле пароля или кода подтверждения запрещён: такие поля заполняет только пользователь";
pub const PAYMENT_FIELD: &str =
    "Ввод в поле платёжных данных запрещён: такие поля заполняет только пользователь";

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FieldFacts {
    pub tag: String,
    pub input_type: String,
    pub name: String,
    pub id: String,
    pub autocomplete: String,
    pub placeholder: String,
    pub aria_label: String,
    pub label: String,
    pub editable: bool,
    pub disabled: bool,
    pub read_only: bool,
    pub in_form: bool,
    pub submits: bool,
    pub href: String,
    pub download: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretField {
    Credential,
    Payment,
}

impl SecretField {
    pub const fn message(self) -> &'static str {
        match self {
            Self::Credential => CREDENTIAL_FIELD,
            Self::Payment => PAYMENT_FIELD,
        }
    }
}

const CREDENTIAL_AUTOCOMPLETE: &[&str] = &[
    "current-password",
    "new-password",
    "one-time-code",
    "webauthn",
];

const CREDENTIAL_WORDS: &[&str] = &[
    "password",
    "passwd",
    "passwort",
    "pwd",
    "pass",
    "passcode",
    "passphrase",
    "pin",
    "otp",
    "totp",
    "hotp",
    "2fa",
    "mfa",
    "пин",
];

const CREDENTIAL_PREFIXES: &[&str] = &["парол", "одноразов"];

const CREDENTIAL_PHRASES: &[&str] = &[
    "onetimecode",
    "onetimepassword",
    "onetimepasscode",
    "verificationcode",
    "confirmationcode",
    "authcode",
    "authenticationcode",
    "smscode",
    "securitykey",
    "кодподтверждения",
    "кодизсмс",
    "смскод",
    "кодизсообщения",
];

const PAYMENT_WORDS: &[&str] = &[
    "cvv", "cvv2", "cvc", "cvc2", "csc", "cvn", "ccnum", "ccnumber", "ccexp", "cccsc", "iban",
];

const PAYMENT_PHRASES: &[&str] = &[
    "cardnumber",
    "cardnum",
    "cardno",
    "creditcard",
    "debitcard",
    "cardholder",
    "nameoncard",
    "cardexpir",
    "expirydate",
    "expirationdate",
    "expdate",
    "securitycode",
    "cardcode",
    "cardverification",
    "номеркарты",
    "данныекарты",
    "банковскаякарта",
    "держателькарты",
    "срокдействиякарты",
    "кодбезопасности",
];

fn words(text: &str) -> Vec<String> {
    let mut words = Vec::new();
    let mut current = String::new();
    let mut previous_lower = false;
    for character in text.chars() {
        if !character.is_alphanumeric() {
            if !current.is_empty() {
                words.push(std::mem::take(&mut current));
            }
            previous_lower = false;
            continue;
        }
        if character.is_uppercase() && previous_lower && !current.is_empty() {
            words.push(std::mem::take(&mut current));
        }
        previous_lower = character.is_lowercase() || character.is_numeric();
        current.extend(character.to_lowercase());
    }
    if !current.is_empty() {
        words.push(current);
    }
    words
}

fn classify(text: &str) -> Option<SecretField> {
    let words = words(text);
    if words.is_empty() {
        return None;
    }
    let joined: String = words.concat();
    let pairs = words.windows(2).map(|pair| pair.concat());
    let payment = words
        .iter()
        .cloned()
        .chain(pairs)
        .any(|word| PAYMENT_WORDS.contains(&word.as_str()))
        || PAYMENT_PHRASES.iter().any(|phrase| joined.contains(phrase));
    if payment {
        return Some(SecretField::Payment);
    }
    let credential = words.iter().any(|word| {
        CREDENTIAL_WORDS.contains(&word.as_str())
            || CREDENTIAL_PREFIXES
                .iter()
                .any(|prefix| word.starts_with(prefix))
    }) || CREDENTIAL_PHRASES
        .iter()
        .any(|phrase| joined.contains(phrase));
    credential.then_some(SecretField::Credential)
}

pub fn secret_field(facts: &FieldFacts) -> Option<SecretField> {
    if facts.tag.eq_ignore_ascii_case("input") && facts.input_type.eq_ignore_ascii_case("password")
    {
        return Some(SecretField::Credential);
    }
    let tokens: Vec<String> = facts
        .autocomplete
        .split_ascii_whitespace()
        .map(str::to_ascii_lowercase)
        .collect();
    if tokens.iter().any(|token| token.starts_with("cc-")) {
        return Some(SecretField::Payment);
    }
    if tokens
        .iter()
        .any(|token| CREDENTIAL_AUTOCOMPLETE.contains(&token.as_str()))
    {
        return Some(SecretField::Credential);
    }
    [
        &facts.name,
        &facts.id,
        &facts.placeholder,
        &facts.aria_label,
        &facts.label,
    ]
    .into_iter()
    .filter_map(|text| classify(text))
    .max_by_key(|field| matches!(field, SecretField::Payment))
}

#[cfg(test)]
mod tests {
    use super::*;

    type Setter = fn(&mut FieldFacts, &str);

    fn input(kind: &str) -> FieldFacts {
        FieldFacts {
            tag: "input".to_owned(),
            input_type: kind.to_owned(),
            editable: true,
            ..FieldFacts::default()
        }
    }

    #[test]
    fn password_inputs_are_refused_whatever_they_are_called() {
        let mut field = input("password");
        field.name = "q".to_owned();
        assert_eq!(secret_field(&field), Some(SecretField::Credential));
        assert_eq!(
            secret_field(&input("PASSWORD")),
            Some(SecretField::Credential)
        );
    }

    #[test]
    fn a_revealed_password_keeps_its_autocomplete_token() {
        let mut field = input("text");
        field.autocomplete = "section-login current-password".to_owned();
        assert_eq!(secret_field(&field), Some(SecretField::Credential));
        field.autocomplete = "new-password".to_owned();
        assert_eq!(secret_field(&field), Some(SecretField::Credential));
        field.autocomplete = "one-time-code".to_owned();
        assert_eq!(secret_field(&field), Some(SecretField::Credential));
    }

    #[test]
    fn credential_names_ids_and_labels_are_recognised() {
        let shapes: [(&str, Setter); 5] = [
            ("name", |field, value| field.name = value.to_owned()),
            ("id", |field, value| field.id = value.to_owned()),
            ("placeholder", |field, value| {
                field.placeholder = value.to_owned();
            }),
            ("aria-label", |field, value| {
                field.aria_label = value.to_owned();
            }),
            ("label", |field, value| field.label = value.to_owned()),
        ];
        for value in [
            "user_password",
            "pwd",
            "loginPassword",
            "pass",
            "PIN",
            "otp-input",
            "2fa_code",
            "verification-code",
            "oneTimeCode",
            "Пароль",
            "Введите пароль ещё раз",
            "Код из СМС",
            "Код подтверждения",
            "Одноразовый код",
        ] {
            for (attribute, set) in shapes {
                let mut field = input("text");
                set(&mut field, value);
                assert_eq!(
                    secret_field(&field),
                    Some(SecretField::Credential),
                    "{attribute}={value}"
                );
            }
        }
    }

    #[test]
    fn payment_fields_are_recognised() {
        let mut stripe = input("tel");
        stripe.name = "cardnumber".to_owned();
        stripe.autocomplete = "cc-number".to_owned();
        assert_eq!(secret_field(&stripe), Some(SecretField::Payment));
        let mut expiry = input("text");
        expiry.placeholder = "MM / YY".to_owned();
        expiry.autocomplete = "billing cc-exp".to_owned();
        assert_eq!(secret_field(&expiry), Some(SecretField::Payment));
        for (name, label) in [
            ("cvc", ""),
            ("card[cvv]", ""),
            ("ccNumber", ""),
            ("checkout_card_number", ""),
            ("", "Credit card number"),
            ("", "Expiration date"),
            ("", "Security code"),
            ("", "Name on card"),
            ("", "Номер карты"),
            ("", "Код безопасности"),
            ("", "CVC/CVV"),
            ("iban", ""),
        ] {
            let mut field = input("text");
            field.name = name.to_owned();
            field.label = label.to_owned();
            assert_eq!(
                secret_field(&field),
                Some(SecretField::Payment),
                "{name}{label}"
            );
        }
    }

    #[test]
    fn ordinary_fields_are_not_refused() {
        for (kind, name, autocomplete, label) in [
            ("email", "email", "username email", "Электронная почта"),
            ("search", "q", "off", "Поиск"),
            ("text", "first_name", "given-name", "Имя"),
            ("text", "spinner", "", ""),
            ("text", "passport_number", "", "Серия и номер паспорта"),
            ("text", "company", "organization", "Компания"),
            ("text", "pinterest", "", "Pinterest"),
            ("text", "expected_delivery", "", "Desired delivery date"),
            ("text", "message", "", "Сообщение"),
            ("text", "promo", "", "Промокод"),
            ("text", "username", "username", "Имя пользователя"),
            ("text", "accNumber", "", "Номер лицевого счёта"),
        ] {
            let mut field = input(kind);
            field.name = name.to_owned();
            field.autocomplete = autocomplete.to_owned();
            field.label = label.to_owned();
            assert_eq!(secret_field(&field), None, "{name}");
        }
        let editor = FieldFacts {
            tag: "div".to_owned(),
            editable: true,
            aria_label: "Текст письма".to_owned(),
            ..FieldFacts::default()
        };
        assert_eq!(secret_field(&editor), None);
    }

    #[test]
    fn camel_case_and_separators_split_into_words() {
        assert_eq!(words("loginPassword"), ["login", "password"]);
        assert_eq!(words("card[cvv]"), ["card", "cvv"]);
        assert_eq!(words("Код из СМС"), ["код", "из", "смс"]);
        assert_eq!(words("2fa_code"), ["2fa", "code"]);
    }
}

use zvs_contract::{AppError, AppErrorCode};

pub const CORRUPT_MESSAGE: &str = "Данные хранилища повреждены";

pub fn corrupt() -> AppError {
    AppError::new(AppErrorCode::DbError, CORRUPT_MESSAGE)
}

pub fn stored_id<T>(parse: fn(&str) -> Option<T>, value: &str) -> Result<T, AppError> {
    parse(value).ok_or_else(corrupt)
}

pub fn unsigned(value: i64) -> Result<u64, AppError> {
    u64::try_from(value).map_err(|_| corrupt())
}

pub fn stored_count(value: impl TryInto<i64>) -> Result<i64, AppError> {
    value.try_into().map_err(|_| corrupt())
}

pub fn widen(value: f32) -> f64 {
    value
        .to_string()
        .parse()
        .unwrap_or_else(|_| f64::from(value))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_widened_float_keeps_its_shortest_decimal_spelling() {
        assert!((widen(0.3) - 0.3).abs() < f64::EPSILON);
        assert!((widen(0.25) - 0.25).abs() < f64::EPSILON);
        assert!((widen(1.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn a_negative_count_is_corrupt() {
        assert_eq!(unsigned(3).expect("fits"), 3);
        assert_eq!(
            unsigned(-1).expect_err("refused").code,
            AppErrorCode::DbError
        );
    }
}

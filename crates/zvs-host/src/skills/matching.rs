use std::collections::HashSet;

const STEM_CHARS: usize = 5;
const MIN_TERM_CHARS: usize = 3;

fn stems(text: &str) -> HashSet<String> {
    text.split(|character: char| !character.is_alphanumeric())
        .filter(|word| word.chars().count() >= MIN_TERM_CHARS)
        .map(|word| word.to_lowercase().chars().take(STEM_CHARS).collect())
        .collect()
}

pub fn lexical_score(message: &str, description: &str) -> f64 {
    let asked = stems(message);
    if asked.is_empty() {
        return 0.0;
    }
    let known = stems(description);
    let shared = asked.intersection(&known).count();
    shared as f64 / asked.len() as f64
}

pub fn cosine(left: &[f32], right: &[f32]) -> f64 {
    if left.len() != right.len() || left.is_empty() {
        return 0.0;
    }
    let (mut dot, mut left_norm, mut right_norm) = (0.0_f64, 0.0_f64, 0.0_f64);
    for (a, b) in left.iter().zip(right) {
        let (a, b) = (f64::from(*a), f64::from(*b));
        dot += a * b;
        left_norm += a * a;
        right_norm += b * b;
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        return 0.0;
    }
    (dot / (left_norm.sqrt() * right_norm.sqrt())).clamp(-1.0, 1.0)
}

pub fn rounded(score: f64) -> f64 {
    if score.is_finite() {
        (score * 1000.0).round() / 1000.0
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lexical_score_ranks_a_matching_phrase_above_an_unrelated_one() {
        let description = "Составляет заметки о выпуске по списку изменений в репозитории";
        let matching = lexical_score("составь заметки к выпуску", description);
        let unrelated = lexical_score("какая погода в москве", description);
        assert!(matching >= 0.6, "{matching}");
        assert!(unrelated.abs() < f64::EPSILON, "{unrelated}");
        assert!(lexical_score("", description).abs() < f64::EPSILON);
        assert!(
            lexical_score(
                "Extract tables from a PDF",
                "Extract text and tables from PDF files"
            ) > 0.9
        );
    }

    #[test]
    fn cosine_is_bounded_and_safe_on_degenerate_vectors() {
        assert!((cosine(&[1.0, 0.0], &[1.0, 0.0]) - 1.0).abs() < 1e-9);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-9);
        assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-9);
        assert!(cosine(&[0.0, 0.0], &[1.0, 0.0]).abs() < f64::EPSILON);
        assert!(cosine(&[1.0], &[1.0, 2.0]).abs() < f64::EPSILON);
        assert!((rounded(0.861_49) - 0.861).abs() < 1e-9);
    }
}

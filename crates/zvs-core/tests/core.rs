use unicode_segmentation::UnicodeSegmentation;
use zvs_core::{
    CancellationToken, Error,
    chunk::{ChunkConfig, chunk_text},
    hash::content_hash,
};

fn chunks(text: &str, size: usize, overlap: usize) -> Vec<&str> {
    chunk_text(
        text,
        ChunkConfig { size, overlap },
        &CancellationToken::new(),
    )
    .unwrap()
    .into_iter()
    .map(|chunk| chunk.text)
    .collect()
}

#[test]
fn empty_and_single_word() {
    assert!(chunks("", 4, 1).is_empty());
    assert!(chunks(" \t\r\n", 4, 1).is_empty());
    assert_eq!(chunks("  hello  ", 4, 1), ["hello"]);
    assert_eq!(chunks("unbrokenlongword", 1, 0), ["unbrokenlongword"]);
}

#[test]
fn overlap_and_hard_limit_without_sentence_breaks() {
    assert_eq!(
        chunks("one two three four five six seven", 3, 1),
        ["one two three", "three four five", "five six seven"]
    );
    assert_eq!(chunks("a b c d e", 2, 0), ["a b", "c d", "e"]);
    assert_eq!(chunks("a b c d", 3, 2), ["a b c", "b c d"]);
    assert_eq!(chunks("a b c", 1, 0), ["a", "b", "c"]);
}

#[test]
fn sentence_boundary_preserves_overlap_and_progress() {
    assert_eq!(
        chunks("One two. Three four five six seven.", 4, 1),
        ["One two.", "two. Three four five", "five six seven."]
    );
    assert_eq!(
        chunks("One. two three four", 3, 1),
        ["One. two three", "three four"]
    );
    assert_eq!(
        chunks("One two!” three four five", 3, 0),
        ["One two!”", "three four five"]
    );
}

#[test]
fn source_offsets_and_whitespace_are_preserved() {
    let text = "  first\tsecond\nthird  fourth  ";
    let result = chunk_text(
        text,
        ChunkConfig {
            size: 2,
            overlap: 1,
        },
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(result[0].text, "first\tsecond");
    assert_eq!(result[1].text, "second\nthird");
    assert_eq!(result[2].text, "third  fourth");
    for chunk in result {
        assert_eq!(chunk.text, &text[chunk.byte_range]);
        assert_eq!(chunk.token_count, 2);
    }
}

#[test]
fn unicode_graphemes_are_never_split() {
    let text = " e\u{301} 👨‍👩‍👧‍👦 👍🏽 🇷🇺 привет 中文。 \u{301}z next ";
    let boundaries: Vec<_> = text
        .grapheme_indices(true)
        .map(|(offset, _)| offset)
        .chain([text.len()])
        .collect();
    let result = chunk_text(
        text,
        ChunkConfig {
            size: 2,
            overlap: 1,
        },
        &CancellationToken::new(),
    )
    .unwrap();
    for chunk in result {
        assert!(boundaries.contains(&chunk.byte_range.start));
        assert!(boundaries.contains(&chunk.byte_range.end));
        assert_eq!(chunk.text, &text[chunk.byte_range]);
    }
    assert_eq!(chunks("你好世界。没有空格", 1, 0), ["你好世界。没有空格"]);
}

#[test]
fn every_size_and_overlap_cover_input_and_terminate() {
    let text = "a b. c d e! f g h i? j k l m n o";
    let words: Vec<_> = text.split_whitespace().collect();
    for size in 1..=20 {
        for overlap in 0..size {
            let result = chunks(text, size, overlap);
            let mut rebuilt = Vec::new();
            for (position, chunk) in result.iter().enumerate() {
                let current: Vec<_> = chunk.split_whitespace().collect();
                assert!(current.len() <= size);
                if position > 0 {
                    assert_eq!(&rebuilt[rebuilt.len() - overlap..], &current[..overlap]);
                }
                rebuilt.extend(
                    current
                        .into_iter()
                        .skip(if position == 0 { 0 } else { overlap }),
                );
            }
            assert_eq!(rebuilt, words);
        }
    }
}

#[test]
fn invalid_configuration_returns_typed_error() {
    for (size, overlap) in [(0, 0), (1, 1), (2, 3), (1, usize::MAX)] {
        assert!(matches!(
            chunk_text("", ChunkConfig { size, overlap }, &CancellationToken::new()),
            Err(Error::InvalidInput(_))
        ));
    }
    assert_eq!(chunks("a b", usize::MAX, 0), ["a b"]);
}

#[test]
fn cancelled_operations_return_no_partial_result() {
    let token = CancellationToken::new();
    let child = token.child_token();
    token.cancel();
    for text in ["", "a b c"] {
        assert!(matches!(
            chunk_text(text, ChunkConfig::default(), &child),
            Err(Error::Cancelled)
        ));
        assert!(matches!(
            content_hash(text.as_bytes(), &child),
            Err(Error::Cancelled)
        ));
    }
}

#[test]
fn hashes_match_blake3_vectors_and_change_with_content() {
    let token = CancellationToken::new();
    assert_eq!(
        content_hash(b"", &token).unwrap(),
        "af1349b9f5f9a1a6a0404dea36dcc9499bcb25c9adc112b7cc9a93cae41f3262"
    );
    assert_eq!(
        content_hash(b"abc", &token).unwrap(),
        "6437b3ac38465133ffb63b75273a8db548c558465d79db03fd359c6cd5bd9d85"
    );
    let binary: Vec<_> = (0..200_000).map(|index| (index % 256) as u8).collect();
    assert_eq!(
        content_hash(&binary, &token).unwrap(),
        blake3::hash(&binary).to_hex().to_string()
    );
    assert_ne!(
        content_hash(b"abc", &token).unwrap(),
        content_hash(b"abd", &token).unwrap()
    );
}

#[test]
fn errors_map_to_shared_app_error_codes() {
    use std::io::ErrorKind;
    let cases = [
        (
            Error::from(std::io::Error::from(ErrorKind::NotFound)),
            "NOT_FOUND",
        ),
        (
            Error::from(std::io::Error::from(ErrorKind::PermissionDenied)),
            "PERMISSION_DENIED",
        ),
        (
            Error::from(std::io::Error::from(ErrorKind::Other)),
            "NATIVE_ERROR",
        ),
        (Error::InvalidInput("size".into()), "VALIDATION_FAILED"),
        (Error::Backend("embedding".into()), "NATIVE_ERROR"),
        (Error::Cancelled, "RUN_CANCELLED"),
    ];
    let shared = include_str!("../../../packages/shared/src/errors/AppErrorCode.ts");
    for (error, code) in cases {
        assert_eq!(error.code(), code);
        assert!(shared.contains(&format!("{code} = \"{code}\"")));
        assert!(!error.to_string().is_empty());
    }
    let error = Error::from(std::io::Error::other("disk"));
    assert!(std::error::Error::source(&error).is_some());
}

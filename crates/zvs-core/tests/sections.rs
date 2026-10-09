use zvs_core::{
    CancellationToken, Error,
    chunk::{ChunkConfig, Segment, chunk_segments, estimate_tokens},
};

fn segment(text: &str, section: &[&str], page: Option<u32>) -> Segment {
    Segment {
        text: text.into(),
        section: section.iter().map(|part| (*part).to_owned()).collect(),
        page,
    }
}

fn texts(segments: &[Segment], size: usize, overlap: usize) -> Vec<String> {
    chunk_segments(
        segments,
        ChunkConfig { size, overlap },
        &CancellationToken::new(),
    )
    .unwrap()
    .into_iter()
    .map(|chunk| chunk.text)
    .collect()
}

#[test]
fn token_estimate_is_script_aware() {
    assert_eq!(estimate_tokens(""), 0);
    assert_eq!(estimate_tokens("   \n\t"), 0);
    assert_eq!(estimate_tokens("abcd"), 1);
    assert_eq!(estimate_tokens("abcde"), 2);
    assert_eq!(estimate_tokens("при"), 1);
    assert_eq!(estimate_tokens("привет"), 2);
    assert_eq!(estimate_tokens("中文字"), 3);
    assert_eq!(estimate_tokens("ab вг 中"), 1 + 1 + 1);
    let russian = "Заявление подаётся в течение десяти дней со дня получения решения.";
    let english = "The application must be submitted within ten days of the decision.";
    assert!(estimate_tokens(russian) >= 18 && estimate_tokens(russian) <= 24);
    assert!(estimate_tokens(english) >= 13 && estimate_tokens(english) <= 18);
}

#[test]
fn chunks_never_cross_a_top_level_section() {
    let segments = [
        segment("Alpha one. Alpha two.", &["A"], None),
        segment("Alpha three.", &["A", "Sub"], None),
        segment("Beta one. Beta two.", &["B"], None),
    ];
    let result = chunk_segments(
        &segments,
        ChunkConfig {
            size: 100,
            overlap: 10,
        },
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(result.len(), 2);
    assert_eq!(result[0].text, "Alpha one. Alpha two.\n\nAlpha three.");
    assert_eq!(result[0].section, ["A"]);
    assert_eq!(result[1].text, "Beta one. Beta two.");
    assert_eq!(result[1].section, ["B"]);
}

#[test]
fn overlap_starts_at_a_sentence_start() {
    let segments = [segment(
        "First sentence here. Second sentence here. Third sentence here. Fourth one.",
        &[],
        None,
    )];
    let result = texts(&segments, 10, 5);
    assert!(result.len() > 1);
    for chunk in &result[1..] {
        let first = chunk.chars().next().unwrap();
        assert!(first.is_uppercase(), "{chunk:?} does not start a sentence");
    }
    assert!(result[0].ends_with("Second sentence here."));
    assert!(result[1].starts_with("Second sentence here."));
}

#[test]
fn zero_overlap_and_every_sentence_is_covered_in_order() {
    let text = "One a. Two b. Three c. Four d. Five e. Six f. Seven g.";
    let segments = [segment(text, &[], None)];
    for size in 2..12 {
        for overlap in 0..size {
            let result = chunk_segments(
                &segments,
                ChunkConfig { size, overlap },
                &CancellationToken::new(),
            )
            .unwrap();
            let mut covered = 0;
            for chunk in &result {
                if chunk.byte_range.start > covered {
                    assert!(text[covered..chunk.byte_range.start].trim().is_empty());
                }
                assert!(chunk.byte_range.end > covered);
                assert_eq!(&text[chunk.byte_range.clone()], chunk.text);
                covered = chunk.byte_range.end;
            }
            assert_eq!(covered, text.len());
            if overlap == 0 {
                for pair in result.windows(2) {
                    assert!(pair[0].byte_range.end <= pair[1].byte_range.start);
                }
            }
        }
    }
}

#[test]
fn size_is_counted_in_estimated_tokens() {
    let text = "Один два три. Четыре пять шесть. Семь восемь девять.";
    let result = chunk_segments(
        &[segment(text, &[], None)],
        ChunkConfig {
            size: 8,
            overlap: 0,
        },
        &CancellationToken::new(),
    )
    .unwrap();
    for chunk in &result {
        assert!(chunk.token_count <= 8);
        assert_eq!(chunk.token_count, estimate_tokens(&chunk.text));
    }
    assert!(result.len() >= 2);
}

#[test]
fn an_oversized_sentence_is_split_by_words() {
    let long = "word ".repeat(40);
    let result = chunk_segments(
        &[segment(long.trim(), &[], None)],
        ChunkConfig {
            size: 5,
            overlap: 2,
        },
        &CancellationToken::new(),
    )
    .unwrap();
    assert_eq!(result.len(), 8);
    assert!(result.iter().all(|chunk| chunk.token_count <= 5));
    assert!(
        result
            .windows(2)
            .all(|pair| pair[0].byte_range.end <= pair[1].byte_range.start)
    );
}

#[test]
fn page_and_section_come_from_where_the_chunk_starts() {
    let segments = [
        segment("Page one text.", &[], Some(1)),
        segment("Page two text.", &[], Some(2)),
        segment("Page three text.", &[], Some(3)),
    ];
    let result = chunk_segments(
        &segments,
        ChunkConfig {
            size: 4,
            overlap: 0,
        },
        &CancellationToken::new(),
    )
    .unwrap();
    let pages: Vec<_> = result.iter().map(|chunk| chunk.page).collect();
    assert_eq!(pages, [Some(1), Some(2), Some(3)]);
    assert_eq!(result[1].text, "Page two text.");
}

#[test]
fn empty_input_invalid_config_and_cancellation() {
    assert!(texts(&[], 4, 1).is_empty());
    assert!(texts(&[segment("  \n ", &["A"], None)], 4, 1).is_empty());
    for (size, overlap) in [(0, 0), (2, 2)] {
        assert!(matches!(
            chunk_segments(
                &[],
                ChunkConfig { size, overlap },
                &CancellationToken::new()
            ),
            Err(Error::InvalidInput(_))
        ));
    }
    let token = CancellationToken::new();
    token.cancel();
    assert!(matches!(
        chunk_segments(
            &[segment("a b c", &[], None)],
            ChunkConfig::default(),
            &token
        ),
        Err(Error::Cancelled)
    ));
}

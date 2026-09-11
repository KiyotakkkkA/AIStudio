use std::ops::Range;

use unicode_segmentation::UnicodeSegmentation;

use crate::{CancellationToken, Error, Result, error::check_cancelled};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChunkConfig {
    pub size: usize,
    pub overlap: usize,
}

impl Default for ChunkConfig {
    fn default() -> Self {
        Self {
            size: 256,
            overlap: 32,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Chunk<'a> {
    pub text: &'a str,
    pub byte_range: Range<usize>,
    pub token_count: usize,
}

pub fn chunk_text<'a>(
    text: &'a str,
    config: ChunkConfig,
    cancellation: &CancellationToken,
) -> Result<Vec<Chunk<'a>>> {
    check_cancelled(cancellation)?;
    if config.size == 0 || config.overlap >= config.size {
        return Err(Error::InvalidInput(
            "Chunk size must be positive and overlap must be smaller than size".into(),
        ));
    }

    let mut tokens: Vec<Range<usize>> = Vec::new();
    let mut word_start = None;
    for (offset, grapheme) in text.grapheme_indices(true) {
        check_cancelled(cancellation)?;
        if grapheme.chars().all(char::is_whitespace) {
            if let Some(start) = word_start.take() {
                tokens.push(start..offset);
            }
        } else {
            word_start.get_or_insert(offset);
        }
    }
    if let Some(start) = word_start {
        tokens.push(start..text.len());
    }

    let mut chunks = Vec::new();
    let mut start = 0;
    while start < tokens.len() {
        check_cancelled(cancellation)?;
        let limit = start.saturating_add(config.size).min(tokens.len());
        let mut end = limit;
        if limit < tokens.len() {
            for candidate in ((start + config.overlap + 1)..=limit).rev() {
                check_cancelled(cancellation)?;
                let word = &text[tokens[candidate - 1].clone()];
                if word
                    .trim_end_matches(['"', '\'', '”', '’', ')', ']', '»'])
                    .ends_with(['.', '!', '?', '。', '！', '？'])
                {
                    end = candidate;
                    break;
                }
            }
        }
        let byte_range = tokens[start].start..tokens[end - 1].end;
        chunks.push(Chunk {
            text: &text[byte_range.clone()],
            byte_range,
            token_count: end - start,
        });
        if end == tokens.len() {
            break;
        }
        start = end - config.overlap;
    }
    check_cancelled(cancellation)?;
    Ok(chunks)
}

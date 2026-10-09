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

pub const SECTION_SEPARATOR: &str = " › ";
const SEGMENT_SEPARATOR: &str = "\n\n";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Segment {
    pub text: String,
    pub section: Vec<String>,
    pub page: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SectionChunk {
    pub text: String,
    pub byte_range: Range<usize>,
    pub token_count: usize,
    pub section: Vec<String>,
    pub page: Option<u32>,
}

pub fn estimate_tokens(text: &str) -> usize {
    let (mut cyrillic, mut ideographic, mut other) = (0usize, 0usize, 0usize);
    for character in text.chars() {
        if character.is_whitespace() {
            continue;
        }
        match u32::from(character) {
            0x0400..=0x052F | 0x1C80..=0x1C8F | 0x2DE0..=0x2DFF | 0xA640..=0xA69F => cyrillic += 1,
            0x1100..=0x11FF
            | 0x3040..=0x30FF
            | 0x3130..=0x318F
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xAC00..=0xD7AF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2FA1F => ideographic += 1,
            _ => other += 1,
        }
    }
    ideographic + cyrillic.div_ceil(3) + other.div_ceil(4)
}

struct Unit {
    range: Range<usize>,
    tokens: usize,
    segment: usize,
    sentence_start: bool,
}

pub fn chunk_segments(
    segments: &[Segment],
    config: ChunkConfig,
    cancellation: &CancellationToken,
) -> Result<Vec<SectionChunk>> {
    check_cancelled(cancellation)?;
    if config.size == 0 || config.overlap >= config.size {
        return Err(Error::InvalidInput(
            "Chunk size must be positive and overlap must be smaller than size".into(),
        ));
    }
    let mut joined = String::new();
    let mut offsets = Vec::with_capacity(segments.len());
    for (index, segment) in segments.iter().enumerate() {
        if index > 0 {
            joined.push_str(SEGMENT_SEPARATOR);
        }
        offsets.push(joined.len());
        joined.push_str(&segment.text);
    }

    let mut chunks = Vec::new();
    let mut group_start = 0;
    while group_start < segments.len() {
        check_cancelled(cancellation)?;
        let top = segments[group_start].section.first();
        let mut group_end = group_start + 1;
        while group_end < segments.len() && segments[group_end].section.first() == top {
            group_end += 1;
        }
        let mut units = Vec::new();
        for index in group_start..group_end {
            sentence_units(
                &segments[index].text,
                offsets[index],
                index,
                config.size,
                &mut units,
                cancellation,
            )?;
        }
        pack(&joined, segments, &units, config, &mut chunks, cancellation)?;
        group_start = group_end;
    }
    check_cancelled(cancellation)?;
    Ok(chunks)
}

fn sentence_units(
    text: &str,
    offset: usize,
    segment: usize,
    size: usize,
    units: &mut Vec<Unit>,
    cancellation: &CancellationToken,
) -> Result<()> {
    for (start, sentence) in text.split_sentence_bound_indices() {
        check_cancelled(cancellation)?;
        let trimmed = sentence.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lead = sentence.len() - sentence.trim_start().len();
        let begin = start + lead;
        let tokens = estimate_tokens(trimmed);
        if tokens <= size {
            units.push(Unit {
                range: offset + begin..offset + begin + trimmed.len(),
                tokens,
                segment,
                sentence_start: true,
            });
            continue;
        }
        let mut words: Vec<Range<usize>> = Vec::new();
        let mut word_start = None;
        for (position, grapheme) in trimmed.grapheme_indices(true) {
            if grapheme.chars().all(char::is_whitespace) {
                if let Some(word) = word_start.take() {
                    words.push(word..position);
                }
            } else {
                word_start.get_or_insert(position);
            }
        }
        if let Some(word) = word_start {
            words.push(word..trimmed.len());
        }
        let mut first = true;
        let mut piece: Option<Range<usize>> = None;
        let mut piece_tokens = 0;
        for word in words {
            check_cancelled(cancellation)?;
            let word_tokens = estimate_tokens(&trimmed[word.clone()]);
            if let Some(current) = &piece
                && piece_tokens + word_tokens > size
            {
                units.push(Unit {
                    range: offset + begin + current.start..offset + begin + current.end,
                    tokens: piece_tokens,
                    segment,
                    sentence_start: first,
                });
                first = false;
                piece = None;
                piece_tokens = 0;
            }
            piece = Some(match piece {
                Some(current) => current.start..word.end,
                None => word,
            });
            piece_tokens += word_tokens;
        }
        if let Some(current) = piece {
            units.push(Unit {
                range: offset + begin + current.start..offset + begin + current.end,
                tokens: piece_tokens,
                segment,
                sentence_start: first,
            });
        }
    }
    Ok(())
}

fn pack(
    joined: &str,
    segments: &[Segment],
    units: &[Unit],
    config: ChunkConfig,
    chunks: &mut Vec<SectionChunk>,
    cancellation: &CancellationToken,
) -> Result<()> {
    let mut start = 0;
    while start < units.len() {
        check_cancelled(cancellation)?;
        let mut end = start;
        let mut tokens = 0;
        while end < units.len() {
            if end > start && tokens + units[end].tokens > config.size {
                break;
            }
            tokens += units[end].tokens;
            end += 1;
        }
        let first = &units[start];
        let byte_range = first.range.start..units[end - 1].range.end;
        let segment = &segments[first.segment];
        chunks.push(SectionChunk {
            text: joined[byte_range.clone()].to_owned(),
            byte_range,
            token_count: tokens,
            section: segment.section.clone(),
            page: segment.page,
        });
        if end == units.len() {
            break;
        }
        let mut next = end;
        let mut carried = 0;
        let mut candidate = end;
        while candidate > start + 1 {
            candidate -= 1;
            carried += units[candidate].tokens;
            if carried > config.overlap || carried + units[end].tokens > config.size {
                break;
            }
            if units[candidate].sentence_start {
                next = candidate;
            }
        }
        start = next;
    }
    Ok(())
}

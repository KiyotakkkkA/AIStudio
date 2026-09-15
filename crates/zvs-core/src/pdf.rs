//! PDF text and page-image extraction.
//!
//! This lives in Rust, and is reached through the **sidecar** rather than the addon, for one
//! reason: a PDF is the least trustworthy input the app accepts. Malformed cross-reference
//! tables, broken object streams and hostile font programs are ordinary in the wild, and the
//! parsers that survive them do so by panicking. A panic here costs one job in a killable child
//! process; in the host it would cost the window.

use std::collections::BTreeSet;
use std::panic::{AssertUnwindSafe, catch_unwind};

// `pdf-extract` re-exports the whole of `lopdf`, so the object model comes from it rather
// than from a second, separately versioned dependency on the same crate.
use pdf_extract::{Dictionary, Document, Object, ObjectId, Stream};
use serde::Serialize;

use crate::error::{Error, Result};

/// One page's text layer. `chars` counts characters rather than bytes so a Cyrillic page is not
/// mistaken for twice the text it has.
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PdfPage {
    /// One-based, matching what a reader shows the user.
    pub number: u32,
    pub text: String,
    pub chars: usize,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfImage {
    pub page: u32,
    pub media_type: String,
    #[serde(skip)]
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PdfNote {
    pub page: u32,
    pub reason: String,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PdfExtraction {
    pub pages: Vec<PdfPage>,
    pub images: Vec<PdfImage>,
    pub notes: Vec<PdfNote>,
}

fn guarded<T>(what: &str, run: impl FnOnce() -> Result<T>) -> Result<T> {
    match catch_unwind(AssertUnwindSafe(run)) {
        Ok(result) => result,
        Err(_) => Err(Error::Backend(format!("{what} panicked on this document"))),
    }
}

pub fn extract_pages(bytes: &[u8]) -> Result<Vec<PdfPage>> {
    guarded("PDF text extraction", || {
        let pages = pdf_extract::extract_text_from_mem_by_pages(bytes)
            .map_err(|error| Error::Backend(format!("Could not read the PDF: {error}")))?;
        Ok(pages
            .into_iter()
            .enumerate()
            .map(|(index, text)| {
                let text = normalise(&text);
                PdfPage {
                    number: u32::try_from(index).unwrap_or(u32::MAX).saturating_add(1),
                    chars: text.chars().count(),
                    text,
                }
            })
            .collect())
    })
}

/// Text for every page, plus the embedded image of each page whose text layer is too thin to be
/// real — a scan. `min_chars` is the store's `minCharsPerPage`.
pub fn extract(bytes: &[u8], min_chars: usize) -> Result<PdfExtraction> {
    let pages = extract_pages(bytes)?;
    let scanned: BTreeSet<u32> = pages
        .iter()
        .filter(|page| page.chars < min_chars)
        .map(|page| page.number)
        .collect();
    if scanned.is_empty() {
        return Ok(PdfExtraction {
            pages,
            images: Vec::new(),
            notes: Vec::new(),
        });
    }
    let (images, notes) = page_images(bytes, &scanned)?;
    Ok(PdfExtraction {
        pages,
        images,
        notes,
    })
}

/// Lifts the page's own image out of the file rather than rendering the page.
///
/// A scanner writes one full-page image per page, so the bytes we want are already there and
/// already a JPEG; pulling them out costs nothing and needs no rasteriser. A page drawn with
/// vectors has no such image, and says so in a note instead of silently yielding nothing.
pub fn page_images(bytes: &[u8], pages: &BTreeSet<u32>) -> Result<(Vec<PdfImage>, Vec<PdfNote>)> {
    guarded("PDF image extraction", || {
        let document = Document::load_mem(bytes)
            .map_err(|error| Error::Backend(format!("Could not read the PDF: {error}")))?;
        let mut images = Vec::new();
        let mut notes = Vec::new();
        for (number, id) in document.get_pages() {
            if !pages.contains(&number) {
                continue;
            }
            match page_image(&document, id) {
                Some((media_type, data)) => images.push(PdfImage {
                    page: number,
                    media_type,
                    bytes: data,
                }),
                None => notes.push(PdfNote {
                    page: number,
                    reason: "на странице нет ни текста, ни встроенного изображения".to_owned(),
                }),
            }
        }
        Ok((images, notes))
    })
}

/// The largest image on the page, which for a scan is the scan itself rather than a logo.
fn page_image(document: &Document, id: ObjectId) -> Option<(String, Vec<u8>)> {
    let page = document.get_dictionary(id).ok()?;
    let resources = resources_of(document, page)?;
    let xobjects = resources
        .get(b"XObject")
        .ok()
        .and_then(|object| resolve_dict(document, object))?;

    let mut best: Option<(u64, String, Vec<u8>)> = None;
    for (_, object) in xobjects.iter() {
        let Ok(reference) = object.as_reference() else {
            continue;
        };
        let Ok(stream) = document.get_object(reference).and_then(Object::as_stream) else {
            continue;
        };
        if stream.dict.get(b"Subtype").and_then(Object::as_name).ok() != Some(b"Image".as_slice()) {
            continue;
        }
        let Some((media_type, data)) = image_bytes(stream) else {
            continue;
        };
        let size = u64::try_from(data.len()).unwrap_or(u64::MAX);
        if best.as_ref().is_none_or(|(seen, _, _)| size > *seen) {
            best = Some((size, media_type, data));
        }
    }
    best.map(|(_, media_type, data)| (media_type, data))
}

/// Resources may sit on the page or be inherited from a parent node of the page tree.
fn resources_of<'a>(document: &'a Document, page: &'a Dictionary) -> Option<&'a Dictionary> {
    if let Ok(object) = page.get(b"Resources")
        && let Some(dictionary) = resolve_dict(document, object)
    {
        return Some(dictionary);
    }
    let mut parent = page.get(b"Parent").ok()?.as_reference().ok()?;
    for _ in 0..16 {
        let node = document.get_dictionary(parent).ok()?;
        if let Ok(object) = node.get(b"Resources")
            && let Some(dictionary) = resolve_dict(document, object)
        {
            return Some(dictionary);
        }
        parent = node.get(b"Parent").ok()?.as_reference().ok()?;
    }
    None
}

fn resolve_dict<'a>(document: &'a Document, object: &'a Object) -> Option<&'a Dictionary> {
    match object {
        Object::Dictionary(dictionary) => Some(dictionary),
        Object::Reference(id) => document.get_dictionary(*id).ok(),
        _ => None,
    }
}

/// Only filters whose payload is already a complete image file are passed through. A raw bitmap
/// would have to be re-encoded, and a vision model cannot read one; those pages get a note.
fn image_bytes(stream: &Stream) -> Option<(String, Vec<u8>)> {
    let filters = stream.filters().ok()?;
    match *filters.last()? {
        b"DCTDecode" => Some(("image/jpeg".to_owned(), stream.content.clone())),
        b"JPXDecode" => Some(("image/jp2".to_owned(), stream.content.clone())),
        _ => None,
    }
}

/// PDF text arrives with the layout's line breaks and a lot of padding. Collapsing runs of blank
/// lines and trailing spaces makes the chunker's job — and the character count — honest.
fn normalise(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut blank = 0usize;
    for line in text.lines() {
        let trimmed = line.trim_end();
        if trimmed.trim().is_empty() {
            blank += 1;
            if blank > 1 {
                continue;
            }
        } else {
            blank = 0;
        }
        out.push_str(trimmed);
        out.push('\n');
    }
    out.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A minimal one-page PDF with a text layer, written by hand so the test owns its input.
    /// Stream lengths and cross-reference offsets are computed rather than counted, because a
    /// PDF whose `/Length` is one byte out parses to an empty page instead of failing loudly.
    fn hello_pdf() -> Vec<u8> {
        let content = "BT /F1 24 Tf 20 100 Td (Hello world) Tj ET
";
        let body = [
            "<< /Type /Catalog /Pages 2 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 200 200] /Contents 4 0 R               /Resources << /Font << /F1 5 0 R >> >> >>"
                .to_owned(),
            format!(
                "<< /Length {} >>
stream
{content}endstream",
                content.len()
            ),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
        ];

        let mut out = String::from("%PDF-1.4
");
        let mut offsets = Vec::new();
        for (index, object) in body.iter().enumerate() {
            offsets.push(out.len());
            out.push_str(&format!("{} 0 obj
{object}
endobj
", index + 1));
        }
        let xref = out.len();
        let size = body.len() + 1;
        out.push_str(&format!("xref
0 {size}
0000000000 65535 f 
"));
        for offset in &offsets {
            out.push_str(&format!("{offset:010} 00000 n 
"));
        }
        out.push_str(&format!(
            "trailer
<< /Size {size} /Root 1 0 R >>
startxref
{xref}
%%EOF
"
        ));
        out.into_bytes()
    }

    #[test]
    fn a_text_layer_is_read_page_by_page() {
        let pages = extract_pages(&hello_pdf()).unwrap();
        assert_eq!(pages.len(), 1);
        assert_eq!(pages[0].number, 1);
        assert!(pages[0].text.contains("Hello world"), "{:?}", pages[0].text);
        assert_eq!(pages[0].chars, pages[0].text.chars().count());
    }

    #[test]
    fn a_page_with_enough_text_is_never_sent_to_ocr() {
        let extraction = extract(&hello_pdf(), 3).unwrap();
        assert!(extraction.images.is_empty());
        assert!(extraction.notes.is_empty());
    }

    #[test]
    fn a_page_that_looks_scanned_but_carries_no_image_says_so() {
        // A high threshold makes the sample page look scanned; it has no image to offer.
        let extraction = extract(&hello_pdf(), 10_000).unwrap();
        assert!(extraction.images.is_empty());
        assert_eq!(extraction.notes.len(), 1);
        assert_eq!(extraction.notes[0].page, 1);
    }

    #[test]
    fn a_file_that_is_not_a_pdf_is_an_error_rather_than_a_panic() {
        let error = extract_pages(b"not a pdf at all").unwrap_err();
        assert_eq!(error.code(), "NATIVE_ERROR");
    }

    #[test]
    fn blank_runs_and_trailing_space_are_collapsed() {
        assert_eq!(normalise("a   \n\n\n\nb  \n"), "a\n\nb");
    }
}

use std::io::Write;

use flate2::Compression;
use flate2::write::DeflateEncoder;
use zvs_contract::AppErrorCode;

use super::*;
use crate::indexing::extraction::{create_extractor_registry, segments_text};

pub(crate) fn zip(members: &[(&str, &str)]) -> Vec<u8> {
    let mut locals = Vec::new();
    let mut central = Vec::new();
    for (name, body) in members {
        let offset = u32::try_from(locals.len()).expect("small");
        let raw = body.as_bytes();
        let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(raw).expect("deflates");
        let stored = encoder.finish().expect("deflates");
        let name_length = u16::try_from(name.len()).expect("short");
        let stored_length = u32::try_from(stored.len()).expect("small");
        let raw_length = u32::try_from(raw.len()).expect("small");

        locals.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        locals.extend_from_slice(&20_u16.to_le_bytes());
        locals.extend_from_slice(&0_u16.to_le_bytes());
        locals.extend_from_slice(&8_u16.to_le_bytes());
        locals.extend_from_slice(&[0; 8]);
        locals.extend_from_slice(&stored_length.to_le_bytes());
        locals.extend_from_slice(&raw_length.to_le_bytes());
        locals.extend_from_slice(&name_length.to_le_bytes());
        locals.extend_from_slice(&0_u16.to_le_bytes());
        locals.extend_from_slice(name.as_bytes());
        locals.extend_from_slice(&stored);

        central.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&8_u16.to_le_bytes());
        central.extend_from_slice(&[0; 8]);
        central.extend_from_slice(&stored_length.to_le_bytes());
        central.extend_from_slice(&raw_length.to_le_bytes());
        central.extend_from_slice(&name_length.to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let count = u16::try_from(members.len()).expect("few");
    let directory_length = u32::try_from(central.len()).expect("small");
    let directory_offset = u32::try_from(locals.len()).expect("small");
    let mut archive = locals;
    archive.extend_from_slice(&central);
    archive.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    archive.extend_from_slice(&[0; 4]);
    archive.extend_from_slice(&count.to_le_bytes());
    archive.extend_from_slice(&count.to_le_bytes());
    archive.extend_from_slice(&directory_length.to_le_bytes());
    archive.extend_from_slice(&directory_offset.to_le_bytes());
    archive.extend_from_slice(&0_u16.to_le_bytes());
    archive
}

fn input(path: &str, bytes: Vec<u8>) -> ExtractionInput {
    ExtractionInput::new(path, bytes)
}

fn segment(text: &str, section: &[&str], page: Option<u32>) -> Segment {
    Segment::new(text, section, page)
}

const DOCX_BODY: &str = r#"<?xml version="1.0"?><w:document><w:body>
      <w:p><w:r><w:t xml:space="preserve">Договор </w:t></w:r><w:r><w:t>№ 42</w:t></w:r></w:p>
      <w:p><w:r><w:t>Сторона A</w:t></w:r><w:tab/><w:r><w:t>ООО &quot;Ромашка&quot;</w:t></w:r></w:p>
      <w:p/>
      <w:p><w:r><w:t>Итого: 100 &lt; 200</w:t></w:r></w:p>
    </w:body></w:document>"#;

#[test]
fn a_docx_is_read_paragraph_by_paragraph_with_entities_decoded() {
    let bytes = zip(&[
        ("word/document.xml", DOCX_BODY),
        ("[Content_Types].xml", "<Types/>"),
    ]);
    let segments = docx_segments(&input("a.docx", bytes)).expect("extracts");
    let text = segments_text(&segments);
    assert_eq!(segments.len(), 1);
    assert!(segments[0].section.is_empty());
    assert!(text.contains("Договор № 42"));
    assert!(text.contains("Сторона A\tООО \"Ромашка\""));
    assert!(text.contains("Итого: 100 < 200"));
}

#[test]
fn a_docx_keeps_the_exact_text_order_and_blank_line_rules() {
    let bytes = zip(&[("word/document.xml", DOCX_BODY)]);
    let segments = docx_segments(&input("a.docx", bytes)).expect("extracts");
    assert_eq!(
        segments,
        [segment(
            "Договор № 42\nСторона A\tООО \"Ромашка\"\nИтого: 100 < 200",
            &[],
            None
        )]
    );
}

#[test]
fn xml_entities_numeric_and_named_alike_survive_the_round_trip() {
    assert_eq!(
        decode_xml_text("&lt;a&gt; &amp; &quot;b&quot; &#1055;&#x440;"),
        "<a> & \"b\" Пр"
    );
    assert_eq!(decode_xml_text("&unknownthing;"), "&unknownthing;");
    assert_eq!(decode_xml_text("&AMP;&nbsp;&#0;"), "& &#0;");
}

#[tokio::test]
async fn an_xlsx_resolves_shared_strings_and_joins_a_row_with_tabs() {
    let bytes = zip(&[
        (
            "xl/sharedStrings.xml",
            "<sst><si><t>Позиция</t></si><si><t>Цена</t></si><si><t>Болт</t></si></sst>",
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet><sheetData>
        <row><c t="s"><v>0</v></c><c t="s"><v>1</v></c></row>
        <row><c t="s"><v>2</v></c><c><v>19.5</v></c></row>
      </sheetData></worksheet>"#,
        ),
    ]);
    let segments = create_extractor_registry()
        .extract(&input("prices.xlsx", bytes))
        .await
        .expect("extracts");
    assert_eq!(segments, [segment("Позиция\tЦена\nБолт\t19.5", &[], None)]);
}

#[test]
fn xlsx_sheets_are_read_in_name_order_with_inline_strings_and_trailing_blanks_dropped() {
    let bytes = zip(&[
        (
            "xl/worksheets/sheet2.xml",
            r#"<worksheet><sheetData><row><c t="inlineStr"><is><t>Второй</t></is></c></row></sheetData></worksheet>"#,
        ),
        (
            "xl/worksheets/sheet1.xml",
            r#"<worksheet><sheetData><row><c><v>1</v></c><c><v></v></c><c t="s"><v>7</v></c></row><row><c><v> </v></c></row></sheetData></worksheet>"#,
        ),
    ]);
    let segments = xlsx_segments(&input("a.xlsx", bytes)).expect("extracts");
    assert_eq!(segments, [segment("1\nВторой", &[], None)]);
}

#[tokio::test]
async fn a_pptx_is_read_slide_by_slide() {
    let bytes = zip(&[
        (
            "ppt/slides/slide2.xml",
            "<p:sld><a:p><a:r><a:t>Вторая</a:t></a:r></a:p></p:sld>",
        ),
        (
            "ppt/slides/slide1.xml",
            "<p:sld><a:p><a:r><a:t>Первая</a:t></a:r></a:p></p:sld>",
        ),
    ]);
    let segments = create_extractor_registry()
        .extract(&input("deck.pptx", bytes))
        .await
        .expect("extracts");
    assert_eq!(
        segments,
        [
            segment("Первая", &[], Some(1)),
            segment("Вторая", &[], Some(2)),
        ]
    );
}

#[test]
fn pptx_slides_sort_by_number_not_by_name_and_empty_slides_keep_their_place() {
    let bytes = zip(&[
        (
            "ppt/slides/slide10.xml",
            "<p:sld><a:p><a:r><a:t>Десятый</a:t></a:r></a:p></p:sld>",
        ),
        ("ppt/slides/slide2.xml", "<p:sld></p:sld>"),
        (
            "ppt/slides/slide1.xml",
            "<p:sld><a:p><a:r><a:t>Один</a:t></a:r></a:p></p:sld>",
        ),
    ]);
    let segments = pptx_segments(&input("deck.pptx", bytes)).expect("extracts");
    assert_eq!(
        segments,
        [
            segment("Один", &[], Some(1)),
            segment("Десятый", &[], Some(3)),
        ]
    );
}

#[tokio::test]
async fn a_pptx_slide_title_becomes_its_section() {
    let bytes = zip(&[(
        "ppt/slides/slide1.xml",
        r#"<p:sld><p:sp><p:nvSpPr><p:nvPr><p:ph type="title"/></p:nvPr></p:nvSpPr>
        <a:p><a:r><a:t>Отпуск</a:t></a:r></a:p></p:sp>
        <p:sp><a:p><a:r><a:t>28 дней в году</a:t></a:r></a:p></p:sp></p:sld>"#,
    )]);
    let segments = create_extractor_registry()
        .extract(&input("deck.pptx", bytes))
        .await
        .expect("extracts");
    assert_eq!(
        segments,
        [segment("Отпуск\n28 дней в году", &["Отпуск"], Some(1))]
    );
}

#[test]
fn docx_headings_by_style_name_or_outline_level_open_nested_sections() {
    let styles = r#"<w:styles>
    <w:style w:type="paragraph" w:styleId="1"><w:name w:val="heading 1"/></w:style>
    <w:style w:type="paragraph" w:styleId="Sub"><w:name w:val="Подзаголовок"/>
      <w:pPr><w:outlineLvl w:val="1"/></w:pPr></w:style>
    <w:style w:type="paragraph" w:styleId="Body"><w:name w:val="Normal"/>
      <w:pPr><w:outlineLvl w:val="9"/></w:pPr></w:style>
  </w:styles>"#;
    assert_eq!(
        heading_levels(styles),
        [("1".to_owned(), 1), ("Sub".to_owned(), 2)]
    );
    let bytes = zip(&[
        (
            "word/document.xml",
            r#"<w:document><w:body>
        <w:p><w:r><w:t>Вступление</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="1"/></w:pPr><w:r><w:t>Отпуск</w:t></w:r></w:p>
        <w:p><w:r><w:t>Общие правила.</w:t></w:r></w:p>
        <w:p><w:pPr><w:pStyle w:val="Sub"/></w:pPr><w:r><w:t>Порядок</w:t></w:r></w:p>
        <w:p><w:r><w:t>Заявление за 14 дней.</w:t></w:r></w:p>
        <w:p><w:pPr><w:outlineLvl w:val="0"/></w:pPr><w:r><w:t>Больничный</w:t></w:r></w:p>
        <w:p><w:r><w:t>Справка.</w:t></w:r></w:p>
      </w:body></w:document>"#,
        ),
        ("word/styles.xml", styles),
    ]);
    let segments = docx_segments(&input("a.docx", bytes)).expect("extracts");
    assert_eq!(
        segments,
        [
            segment("Вступление", &[], None),
            segment("Отпуск\nОбщие правила.", &["Отпуск"], None),
            segment(
                "Порядок\nЗаявление за 14 дней.",
                &["Отпуск", "Порядок"],
                None
            ),
            segment("Больничный\nСправка.", &["Больничный"], None),
        ]
    );
}

#[test]
fn a_style_missing_from_the_styles_part_falls_back_to_its_name() {
    let bytes = zip(&[(
        "word/document.xml",
        r#"<w:document><w:body>
        <w:p><w:pPr><w:pStyle w:val="Заголовок2"/></w:pPr><w:r><w:t>Раздел</w:t></w:r></w:p>
        <w:p><w:r><w:t>Текст.</w:t></w:r></w:p>
      </w:body></w:document>"#,
    )]);
    assert_eq!(
        docx_segments(&input("a.docx", bytes)).expect("extracts"),
        [segment("Раздел\nТекст.", &["Раздел"], None)]
    );
}

#[test]
fn a_file_that_is_not_a_zip_fails_as_an_unsupported_format_not_a_crash() {
    let error =
        docx_segments(&input("a.docx", b"plain text, not a zip".to_vec())).expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    let empty = zip(&[("other.xml", "<x/>")]);
    let error = pptx_segments(&input("a.pptx", empty)).expect_err("refuses");
    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    assert_eq!(error.message, "В файле нет части ppt/slides");
}

#[test]
fn tidy_collapses_padding_but_keeps_tabs_and_single_blank_lines() {
    let lines: Vec<String> = ["", "a   b\t", "", "", "c\td", "  ", "e", ""]
        .iter()
        .map(|line| (*line).to_owned())
        .collect();
    assert_eq!(tidy(&lines), "a b\n\nc\td\n\ne");
}

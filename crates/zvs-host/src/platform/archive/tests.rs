use std::io::Write;

use flate2::Compression;
use flate2::write::{DeflateEncoder, GzEncoder};

use super::*;
use crate::testing::temporary_directory;

struct Member<'a> {
    name: &'a str,
    body: Vec<u8>,
    deflate: bool,
    unix_mode: Option<u32>,
}

fn stored<'a>(name: &'a str, body: &[u8]) -> Member<'a> {
    Member {
        name,
        body: body.to_vec(),
        deflate: false,
        unix_mode: None,
    }
}

fn deflated<'a>(name: &'a str, body: &[u8]) -> Member<'a> {
    Member {
        deflate: true,
        ..stored(name, body)
    }
}

fn build_zip(members: &[Member<'_>]) -> Vec<u8> {
    let mut locals = Vec::new();
    let mut central = Vec::new();
    for member in members {
        let name = member.name.as_bytes();
        let data = if member.deflate {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&member.body).expect("deflates");
            encoder.finish().expect("deflates")
        } else {
            member.body.clone()
        };
        let method: u16 = if member.deflate { 8 } else { 0 };
        let offset = u32::try_from(locals.len()).expect("small");
        let data_length = u32::try_from(data.len()).expect("small");
        let body_length = u32::try_from(member.body.len()).expect("small");
        let name_length = u16::try_from(name.len()).expect("short");

        locals.extend_from_slice(&0x0403_4b50_u32.to_le_bytes());
        locals.extend_from_slice(&20_u16.to_le_bytes());
        locals.extend_from_slice(&0_u16.to_le_bytes());
        locals.extend_from_slice(&method.to_le_bytes());
        locals.extend_from_slice(&[0; 8]);
        locals.extend_from_slice(&data_length.to_le_bytes());
        locals.extend_from_slice(&body_length.to_le_bytes());
        locals.extend_from_slice(&name_length.to_le_bytes());
        locals.extend_from_slice(&0_u16.to_le_bytes());
        locals.extend_from_slice(name);
        locals.extend_from_slice(&data);

        let made_by: u16 = if member.unix_mode.is_some() {
            (3 << 8) | 20
        } else {
            20
        };
        central.extend_from_slice(&0x0201_4b50_u32.to_le_bytes());
        central.extend_from_slice(&made_by.to_le_bytes());
        central.extend_from_slice(&20_u16.to_le_bytes());
        central.extend_from_slice(&0_u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0; 8]);
        central.extend_from_slice(&data_length.to_le_bytes());
        central.extend_from_slice(&body_length.to_le_bytes());
        central.extend_from_slice(&name_length.to_le_bytes());
        central.extend_from_slice(&[0; 8]);
        central.extend_from_slice(&(member.unix_mode.unwrap_or(0) << 16).to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name);
    }
    let count = u16::try_from(members.len()).expect("few");
    let mut out = locals.clone();
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50_u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&count.to_le_bytes());
    out.extend_from_slice(&u32::try_from(central.len()).expect("small").to_le_bytes());
    out.extend_from_slice(&u32::try_from(locals.len()).expect("small").to_le_bytes());
    out.extend_from_slice(&0_u16.to_le_bytes());
    out
}

fn raw_header(name: &[u8], size: u64, kind: tar::EntryType, link: &[u8]) -> tar::Header {
    let mut header = tar::Header::new_gnu();
    header.as_mut_bytes()[..name.len()].copy_from_slice(name);
    header.as_mut_bytes()[157..157 + link.len()].copy_from_slice(link);
    header.set_size(size);
    header.set_mode(0o755);
    header.set_entry_type(kind);
    header.set_cksum();
    header
}

fn tar_gz(build: impl FnOnce(&mut tar::Builder<GzEncoder<Vec<u8>>>)) -> Vec<u8> {
    let mut builder = tar::Builder::new(GzEncoder::new(Vec::new(), Compression::default()));
    build(&mut builder);
    builder
        .into_inner()
        .expect("finishes")
        .finish()
        .expect("compresses")
}

fn append(builder: &mut tar::Builder<GzEncoder<Vec<u8>>>, name: &[u8], body: &[u8]) {
    let header = raw_header(name, body.len() as u64, tar::EntryType::Regular, b"");
    builder.append(&header, body).expect("appends");
}

fn append_link(
    builder: &mut tar::Builder<GzEncoder<Vec<u8>>>,
    name: &[u8],
    kind: tar::EntryType,
    link: &[u8],
) {
    let header = raw_header(name, 0, kind, link);
    builder.append(&header, io::empty()).expect("appends");
}

fn files_under(root: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = std::fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(files_under(&path));
        } else {
            found.push(path);
        }
    }
    found.sort();
    found
}

#[tokio::test]
async fn a_zip_is_unpacked_stored_and_deflated_members_alike() {
    let workspace = temporary_directory("archive-");
    let archive = workspace.join("build.zip");
    std::fs::write(
        &archive,
        build_zip(&[
            stored("llama-server.exe", b"binary-ish"),
            deflated("lib/ggml.dll", "x".repeat(4096).as_bytes()),
        ]),
    )
    .expect("writes");
    let target = workspace.join("out");

    let report = extract_archive(&archive, &target, ExtractOptions::default())
        .await
        .expect("extracts");

    assert_eq!(report.files, 2);
    assert_eq!(report.bytes, 10 + 4096);
    assert_eq!(
        std::fs::read_to_string(target.join("llama-server.exe")).expect("reads"),
        "binary-ish"
    );
    assert_eq!(
        std::fs::read_to_string(target.join("lib").join("ggml.dll")).expect("reads"),
        "x".repeat(4096)
    );
}

#[tokio::test]
async fn a_tar_gz_is_unpacked_including_a_file_that_needs_several_blocks() {
    let workspace = temporary_directory("archive-");
    let archive = workspace.join("build.tar.gz");
    std::fs::write(
        &archive,
        tar_gz(|builder| {
            append(builder, b"build/bin/llama-server", b"elf");
            append(
                builder,
                b"build/bin/libggml.so",
                "y".repeat(3000).as_bytes(),
            );
            append_link(
                builder,
                b"build/bin/link",
                tar::EntryType::Symlink,
                b"llama-server",
            );
        }),
    )
    .expect("writes");
    let target = workspace.join("out");
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let sink = Arc::clone(&seen);

    let report = extract_archive(
        &archive,
        &target,
        ExtractOptions {
            cancel: None,
            on_entry: Some(Arc::new(move |name: &str, done, total| {
                sink.lock()
                    .expect("not poisoned")
                    .push((name.to_owned(), done, total));
            })),
        },
    )
    .await
    .expect("extracts");

    assert_eq!(report.files, 2);
    let bin = target.join("build").join("bin");
    assert_eq!(
        std::fs::read_to_string(bin.join("llama-server")).expect("reads"),
        "elf"
    );
    assert_eq!(
        std::fs::read(bin.join("libggml.so")).expect("reads").len(),
        3000
    );
    assert!(!bin.join("link").exists());
    let seen = seen.lock().expect("not poisoned").clone();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].1, 2);
    assert_eq!(seen[1].2, 0);
}

#[test]
fn a_member_that_points_outside_the_target_is_refused_not_written() {
    let workspace = temporary_directory("archive-");
    let target = workspace.join("out");

    assert_eq!(safe_member_path(&target, "../escape.exe"), None);
    assert_eq!(safe_member_path(&target, "a/../../escape.exe"), None);
    assert_eq!(
        safe_member_path(&target, r"C:\windows\system32\evil.dll"),
        None
    );
    assert_eq!(safe_member_path(&target, "c:evil.dll"), None);
    assert_eq!(safe_member_path(&target, "nested/"), None);
    assert_eq!(safe_member_path(&target, ""), None);
    assert_eq!(safe_member_path(&target, "./."), None);
    assert_eq!(
        safe_member_path(&target, "/etc/passwd"),
        Some(target.join("etc").join("passwd"))
    );
    assert_eq!(
        safe_member_path(&target, r"\\server\share\x"),
        Some(target.join("server").join("share").join("x"))
    );
    assert_eq!(
        safe_member_path(&target, "bin/llama-server"),
        Some(target.join("bin").join("llama-server"))
    );
    assert_eq!(
        safe_member_path(&target, "bin/./tools/../llama-server"),
        Some(target.join("bin").join("llama-server"))
    );
}

#[tokio::test]
async fn a_traversing_zip_member_leaves_nothing_behind_it() {
    let workspace = temporary_directory("archive-");
    let archive = workspace.join("evil.zip");
    std::fs::write(
        &archive,
        build_zip(&[
            stored("../escaped.txt", b"no"),
            stored("sub/../../../escaped-too.txt", b"no"),
            stored(r"C:\escaped.txt", b"no"),
            stored("kept.txt", b"yes"),
        ]),
    )
    .expect("writes");
    let target = workspace.join("out");

    let report = extract_archive(&archive, &target, ExtractOptions::default())
        .await
        .expect("extracts");

    assert_eq!(report.files, 1);
    assert_eq!(
        files_under(workspace.path()),
        [archive, target.join("kept.txt")]
    );
}

#[tokio::test]
async fn a_traversing_or_linking_tar_member_leaves_nothing_behind_it() {
    let workspace = temporary_directory("archive-");
    let archive = workspace.join("evil.tar.gz");
    std::fs::write(
        &archive,
        tar_gz(|builder| {
            append(builder, b"../escaped.txt", b"no");
            append(builder, b"/abs/rooted.txt", b"rooted");
            append_link(builder, b"passwd", tar::EntryType::Symlink, b"/etc/passwd");
            append_link(builder, b"hard", tar::EntryType::Link, b"../escaped.txt");
            append(builder, b"passwd", b"overwritten through a link?");
        }),
    )
    .expect("writes");
    let target = workspace.join("out");

    let report = extract_archive(&archive, &target, ExtractOptions::default())
        .await
        .expect("extracts");

    assert_eq!(report.files, 2);
    assert_eq!(
        files_under(workspace.path()),
        [
            archive,
            target.join("abs").join("rooted.txt"),
            target.join("passwd"),
        ]
    );
    let passwd = target.join("passwd");
    assert!(
        !std::fs::symlink_metadata(&passwd)
            .expect("exists")
            .file_type()
            .is_symlink()
    );
    assert_eq!(
        std::fs::read_to_string(passwd).expect("reads"),
        "overwritten through a link?"
    );
}

#[tokio::test]
async fn a_zip_symlink_member_is_written_as_a_plain_file_never_a_link() {
    let workspace = temporary_directory("archive-");
    let archive = workspace.join("link.zip");
    std::fs::write(
        &archive,
        build_zip(&[Member {
            unix_mode: Some(0o120_777),
            ..stored("lib/libggml.so", b"../../../../etc/passwd")
        }]),
    )
    .expect("writes");
    let target = workspace.join("out");

    extract_archive(&archive, &target, ExtractOptions::default())
        .await
        .expect("extracts");

    let written = target.join("lib").join("libggml.so");
    assert!(
        std::fs::symlink_metadata(&written)
            .expect("exists")
            .file_type()
            .is_file()
    );
    assert_eq!(
        std::fs::read_to_string(written).expect("reads"),
        "../../../../etc/passwd"
    );
}

#[tokio::test]
async fn an_overlong_member_name_lands_past_the_windows_path_limit() {
    let workspace = temporary_directory("archive-");
    let segment = "очень-длинное-имя-каталога-сборки-движка";
    let long_name = [segment; 6].join("/") + "/llama-server";
    assert!(long_name.len() > 100);
    let target = workspace.join("out");
    assert!(target.join(&long_name).to_string_lossy().chars().count() > 260);

    let tarball = workspace.join("long.tar.gz");
    std::fs::write(
        &tarball,
        tar_gz(|builder| {
            let mut header = tar::Header::new_gnu();
            header.set_size(3);
            header.set_mode(0o755);
            builder
                .append_data(&mut header, &long_name, &b"elf"[..])
                .expect("appends a GNU long name");
        }),
    )
    .expect("writes");
    let report = extract_archive(&tarball, &target, ExtractOptions::default())
        .await
        .expect("extracts");
    assert_eq!(report.files, 1);
    let landed = target.join(&long_name);
    assert_eq!(std::fs::read_to_string(&landed).expect("reads"), "elf");

    let zip = workspace.join("long.zip");
    std::fs::write(&zip, build_zip(&[stored(&long_name, b"zip")])).expect("writes");
    let zipped = workspace.join("zipped");
    extract_archive(&zip, &zipped, ExtractOptions::default())
        .await
        .expect("extracts");
    assert_eq!(
        std::fs::read_to_string(zipped.join(&long_name)).expect("reads"),
        "zip"
    );
}

#[tokio::test]
async fn an_unknown_archive_extension_is_refused_rather_than_guessed_at() {
    let workspace = temporary_directory("archive-");
    let archive = workspace.join("build.7z");
    std::fs::write(&archive, b"not an archive we read").expect("writes");

    let error = extract_archive(&archive, &workspace.join("out"), ExtractOptions::default())
        .await
        .expect_err("refuses");

    assert_eq!(error.code, AppErrorCode::UnsupportedFormat);
    assert_eq!(error.message, UNKNOWN_FORMAT);
    assert!(
        error
            .details
            .is_some_and(|details| details.contains_key("archivePath"))
    );
}

#[test]
fn a_damaged_zip64_or_exotic_zip_is_refused_with_its_reason() {
    let workspace = temporary_directory("archive-");
    let target = workspace.join("out");
    let refuse = |bytes: Vec<u8>| {
        let archive = workspace.join("bad.zip");
        std::fs::write(&archive, bytes).expect("writes");
        extract_archive_blocking(&archive, &target, &ExtractOptions::default())
            .expect_err("refuses")
    };

    let missing = refuse(b"PK but no directory".to_vec());
    assert_eq!(missing.code, AppErrorCode::UnsupportedFormat);
    assert_eq!(missing.message, NO_DIRECTORY);

    let mut zip64 = build_zip(&[stored("a.txt", b"a")]);
    let eocd = zip64.len() - 22;
    let mut locator = 0x0706_4b50_u32.to_le_bytes().to_vec();
    locator.extend_from_slice(&[0; 16]);
    zip64.splice(eocd..eocd, locator);
    assert_eq!(refuse(zip64).message, ZIP64_REFUSED);

    let mut bzip = build_zip(&[stored("a.txt", b"a")]);
    let central = bzip.len() - 22 - 46 - 5;
    bzip[central + 10] = 12;
    let error = refuse(bzip);
    assert_eq!(error.message, UNSUPPORTED_METHOD);
    assert_eq!(
        error
            .details
            .and_then(|details| details.get("method").cloned()),
        Some(serde_json::json!(12))
    );

    let mut cut = build_zip(&[stored("a.txt", b"abc")]);
    let central = cut.len() - 22 - 46 - 5;
    cut[central + 20] = 200;
    assert_eq!(refuse(cut).message, TRUNCATED);
    assert!(!target.exists());
}

#[tokio::test]
async fn a_cancelled_extraction_writes_nothing() {
    let workspace = temporary_directory("archive-");
    let archive = workspace.join("build.zip");
    std::fs::write(&archive, build_zip(&[stored("a.txt", b"a")])).expect("writes");
    let cancel = CancellationToken::new();
    cancel.cancel();

    let error = extract_archive(
        &archive,
        &workspace.join("out"),
        ExtractOptions {
            cancel: Some(cancel),
            on_entry: None,
        },
    )
    .await
    .expect_err("cancelled");

    assert_eq!(error.code, AppErrorCode::RunCancelled);
    assert!(!workspace.join("out").exists());
}

#[test]
fn wanted_zip_entries_are_read_from_memory() {
    let bytes = build_zip(&[
        deflated("word/document.xml", b"<w:document/>"),
        stored("word/", b""),
        stored("docProps/app.xml", b"<app/>"),
    ]);

    let found = read_zip_entries(&bytes, |name| name.starts_with("word/")).expect("reads");

    assert_eq!(
        found,
        [("word/document.xml".to_owned(), b"<w:document/>".to_vec())]
    );
    assert_eq!(
        read_zip_entries(b"nothing", |_| true)
            .expect_err("refuses")
            .message,
        NO_DIRECTORY
    );
}

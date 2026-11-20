//! Property tests for the mesh readers: damaged copies of the sample files and plain random
//! bytes. Whatever a file holds, opening it gives parts that hang together or a message, never
//! a crash (in the web build a crash ends the worker). A failure prints the smallest input that
//! breaks the rule and saves it beside this file (`prop_readers.proptest-regressions`, to
//! commit), so it is tried first on every later run.
#![cfg(not(target_arch = "wasm32"))]

use proptest::prelude::*;
use proptest::sample::{Index, select};
use proptest::test_runner::FileFailurePersistence;
use std::io::{Cursor, Read, Write};
use std::path::PathBuf;
use std::sync::LazyLock;
use stl_io::{MESH_EXTS, Part, ReadError, load_mesh_file};

/// The sample files of the reader tests: `(file name, bytes)`.
static SEEDS: LazyLock<Vec<(String, Vec<u8>)>> = LazyLock::new(|| {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests/models/readers");
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .map(|f| {
            let f = f.unwrap();
            (
                f.file_name().to_string_lossy().into_owned(),
                std::fs::read(f.path()).unwrap(),
            )
        })
        .collect();
    out.sort();
    assert!(out.len() >= 8, "sample files are missing from {}", dir.display());
    out
});

/// Numbers that readers trip over when a count or a coordinate is replaced by them.
const NUMBERS: &[&str] = &[
    "0",
    "-1",
    "1",
    "3",
    "4",
    "255",
    "65536",
    "4294967295",
    "4294967296",
    "99999999999999999999",
    "1e999",
    "-1e999",
    "nan",
    "inf",
    "0.5",
    "-0",
    "1e-320",
    "",
];

#[derive(Debug, Clone)]
enum Edit {
    /// Overwrite one byte.
    Set(Index, u8),
    /// Remove a few bytes.
    Cut(Index, u8),
    /// Put a few bytes in.
    Insert(Index, Vec<u8>),
    /// Drop everything after a point.
    Truncate(Index),
    /// Write a run of bytes a second time.
    Repeat(Index, u8),
    /// Replace one of the numbers written as text.
    Number(Index, &'static str),
}

fn edit() -> impl Strategy<Value = Edit> {
    prop_oneof![
        4 => (any::<Index>(), any::<u8>()).prop_map(|(i, b)| Edit::Set(i, b)),
        2 => (any::<Index>(), 1u8..40).prop_map(|(i, n)| Edit::Cut(i, n)),
        2 => (any::<Index>(), prop::collection::vec(any::<u8>(), 1..12)).prop_map(|(i, b)| Edit::Insert(i, b)),
        1 => any::<Index>().prop_map(Edit::Truncate),
        1 => (any::<Index>(), 1u8..80).prop_map(|(i, n)| Edit::Repeat(i, n)),
        4 => (any::<Index>(), select(NUMBERS)).prop_map(|(i, s)| Edit::Number(i, s)),
    ]
}

fn apply(data: &mut Vec<u8>, e: &Edit) {
    match e {
        Edit::Set(i, b) => {
            if !data.is_empty() {
                let at = i.index(data.len());
                data[at] = *b;
            }
        }
        Edit::Cut(i, n) => {
            if !data.is_empty() {
                let at = i.index(data.len());
                let end = (at + *n as usize).min(data.len());
                data.drain(at..end);
            }
        }
        Edit::Insert(i, bytes) => {
            let at = i.index(data.len() + 1);
            data.splice(at..at, bytes.iter().copied());
        }
        Edit::Truncate(i) => data.truncate(i.index(data.len() + 1)),
        Edit::Repeat(i, n) => {
            if !data.is_empty() {
                let at = i.index(data.len());
                let end = (at + *n as usize).min(data.len());
                let run = data[at..end].to_vec();
                data.splice(end..end, run);
            }
        }
        Edit::Number(i, text) => {
            // the runs of ASCII digits, as (start, end)
            let mut runs = Vec::new();
            let mut k = 0;
            while k < data.len() {
                if data[k].is_ascii_digit() {
                    let start = k;
                    while k < data.len() && data[k].is_ascii_digit() {
                        k += 1;
                    }
                    runs.push((start, k));
                } else {
                    k += 1;
                }
            }
            if !runs.is_empty() {
                let (start, end) = runs[i.index(runs.len())];
                data.splice(start..end, text.bytes());
            }
        }
    }
}

fn damaged(data: &[u8], edits: &[Edit]) -> Vec<u8> {
    let mut out = data.to_vec();
    for e in edits {
        apply(&mut out, e);
    }
    out
}

/// A 3MF is a zip: damage the files inside it and zip them again, so the damage reaches the XML
/// reader instead of stopping at the zip's checksum.
fn damaged_zip(data: &[u8], edits: &[Edit]) -> Option<Vec<u8>> {
    let mut z = zip::ZipArchive::new(Cursor::new(data)).ok()?;
    let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    for i in 0..z.len() {
        let mut f = z.by_index(i).ok()?;
        let name = f.name().ok()?.to_string();
        let mut bytes = Vec::new();
        f.read_to_end(&mut bytes).ok()?;
        w.start_file(name, opts).ok()?;
        w.write_all(&damaged(&bytes, edits)).ok()?;
    }
    Some(w.finish().ok()?.into_inner())
}

/// What a file that opened must look like.
fn check(result: Result<Vec<Part>, ReadError>) -> Result<(), TestCaseError> {
    match result {
        Err(e) => prop_assert!(!e.0.is_empty(), "an error without a message"),
        Ok(parts) => {
            prop_assert!(!parts.is_empty(), "opened with no parts");
            for p in &parts {
                prop_assert!(
                    p.positions.len() % 3 == 0,
                    "{}: {} coordinates",
                    p.name,
                    p.positions.len()
                );
                prop_assert!(
                    p.triangles.len() % 3 == 0,
                    "{}: {} corner indices",
                    p.name,
                    p.triangles.len()
                );
                let points = p.positions.len() / 3;
                let worst = p.triangles.iter().copied().max().unwrap_or(0) as usize;
                prop_assert!(
                    p.triangles.is_empty() || worst < points,
                    "{}: corner {worst} of {points} points",
                    p.name
                );
            }
        }
    }
    Ok(())
}

/// Failing inputs are saved beside this file (the path is from the crate folder, where Cargo runs tests).
fn config(cases: u32) -> ProptestConfig {
    ProptestConfig {
        cases,
        failure_persistence: Some(Box::new(FileFailurePersistence::Direct(
            "tests/prop_readers.proptest-regressions",
        ))),
        ..ProptestConfig::default()
    }
}

fn ext_of(name: &str) -> &str {
    &name[name.rfind('.').unwrap_or(0)..]
}

proptest! {
    #![proptest_config(config(2000))]

    /// A damaged sample file opens properly or gives a message.
    #[test]
    fn damaged_files_open_or_give_a_message(
        seed in any::<Index>(),
        edits in prop::collection::vec(edit(), 1..5),
    ) {
        let (name, data) = &SEEDS[seed.index(SEEDS.len())];
        check(load_mesh_file(&damaged(data, &edits), name))?;
        if ext_of(name) == ".3mf"
            && let Some(zipped) = damaged_zip(data, &edits)
        {
            check(load_mesh_file(&zipped, name))?;
        }
    }

    /// A sample file opened as another format (a wrong extension) gives parts or a message.
    #[test]
    fn a_wrong_extension_gives_a_message(
        seed in any::<Index>(),
        ext in select(&MESH_EXTS[..]),
        edits in prop::collection::vec(edit(), 0..3),
    ) {
        let (_, data) = &SEEDS[seed.index(SEEDS.len())];
        check(load_mesh_file(&damaged(data, &edits), &format!("part{ext}")))?;
    }

    /// Random bytes under every extension give parts or a message.
    #[test]
    fn random_bytes_give_a_message(
        data in prop::collection::vec(any::<u8>(), 0..600),
        ext in select(&MESH_EXTS[..]),
    ) {
        check(load_mesh_file(&data, &format!("x{ext}")))?;
    }

    /// Random text (words the text formats use, numbers, line ends) gives parts or a message.
    #[test]
    fn random_text_gives_a_message(
        words in prop::collection::vec(
            prop_oneof![
                select(&[
                    "solid", "facet", "normal", "outer", "loop", "vertex", "endloop", "endfacet", "endsolid", "v", "f",
                    "vn", "vt", "o", "g", "usemtl", "OFF", "COFF", "ply", "format", "ascii", "binary_little_endian",
                    "1.0", "element", "face", "property", "float", "list", "uchar", "int", "vertex_indices", "end_header",
                    "x", "y", "z", "#", "/", "//", "1/2/3", "-1", "\n", "\r\n", "\t",
                ][..]).prop_map(String::from),
                select(NUMBERS).prop_map(String::from),
                (-100i32..100).prop_map(|n| n.to_string()),
                (-1e3f64..1e3).prop_map(|x| format!("{x:.3}")),
            ],
            0..80,
        ),
        ext in select(&[".stl", ".obj", ".ply", ".off"][..]),
    ) {
        check(load_mesh_file(words.join(" ").as_bytes(), &format!("x{ext}")))?;
    }
}

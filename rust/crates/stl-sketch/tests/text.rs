//! Text contours and extent against `rust/tests/parity/text_golden.json`.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;
use stl_sketch::text::{TextOpts, text_contours, text_extent};

fn tests_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../tests")
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../..")
}

fn bundled_font_path(family: &str, bold: bool, italic: bool) -> PathBuf {
    // Same style resolution as meshtools._font_path against web/fonts DejaVu files.
    let fonts = repo_root().join("web/fonts");
    let file = match (family, bold, italic) {
        ("DejaVu Sans", true, true) => "DejaVuSans-BoldOblique.ttf",
        ("DejaVu Sans", true, false) => "DejaVuSans-Bold.ttf",
        ("DejaVu Sans", false, true) => "DejaVuSans-Oblique.ttf",
        ("DejaVu Sans", false, false) => "DejaVuSans.ttf",
        ("DejaVu Sans Mono", true, _) => "DejaVuSansMono-Bold.ttf",
        ("DejaVu Sans Mono", false, _) => "DejaVuSansMono.ttf",
        ("DejaVu Serif", true, _) => "DejaVuSerif-Bold.ttf",
        ("DejaVu Serif", false, _) => "DejaVuSerif.ttf",
        _ => panic!("unexpected text golden family {family:?} bold={bold} italic={italic}"),
    };
    fonts.join(file)
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9
}

fn contours_match(got: &[Vec<[f64; 2]>], exp: &Value) -> Result<(), String> {
    let exp = exp.as_array().ok_or("contours not array")?;
    if got.len() != exp.len() {
        return Err(format!("contour count {} != {}", got.len(), exp.len()));
    }
    for (ci, (gc, ec)) in got.iter().zip(exp.iter()).enumerate() {
        let ec = ec.as_array().ok_or_else(|| format!("contour {ci} not array"))?;
        if gc.len() != ec.len() {
            return Err(format!(
                "contour {ci} len {} != {}",
                gc.len(),
                ec.len()
            ));
        }
        for (pi, (gp, ep)) in gc.iter().zip(ec.iter()).enumerate() {
            let ep = ep.as_array().ok_or_else(|| format!("pt {ci}.{pi}"))?;
            let ex = ep[0].as_f64().unwrap();
            let ey = ep[1].as_f64().unwrap();
            if !near(gp[0], ex) || !near(gp[1], ey) {
                return Err(format!(
                    "contour {ci} pt {pi}: got [{}, {}] expected [{ex}, {ey}]",
                    gp[0], gp[1]
                ));
            }
        }
    }
    Ok(())
}

fn run_case(case_path: &Path, golden: &Value) -> Result<(), String> {
    let case: Value = serde_json::from_str(&fs::read_to_string(case_path).unwrap())
        .map_err(|e| e.to_string())?;
    let name = case
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or_else(|| {
            case_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("case")
        });
    let exp = golden
        .get(name)
        .ok_or_else(|| format!("missing golden entry {name}"))?;

    if let Some(err) = exp.get("error").and_then(|v| v.as_str()) {
        let font = fs::read(bundled_font_path(
            case.get("family").and_then(|v| v.as_str()).unwrap_or(""),
            case.get("bold").and_then(|v| v.as_bool()).unwrap_or(false),
            case.get("italic").and_then(|v| v.as_bool()).unwrap_or(false),
        ))
        .map_err(|e| e.to_string())?;
        let opts = TextOpts {
            spacing: case.get("spacing").and_then(|v| v.as_f64()).unwrap_or(0.0),
            align: case
                .get("align")
                .and_then(|v| v.as_str())
                .unwrap_or("left")
                .to_string(),
            line_gap: case
                .get("line_gap")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.2),
        };
        let size = case.get("size").and_then(|v| v.as_f64()).unwrap_or(10.0);
        let text = case.get("text").and_then(|v| v.as_str()).unwrap_or("");
        match text_contours(&font, text, size, &opts) {
            Err(e) if e.message() == err => Ok(()),
            Err(e) => Err(format!("error {:?} != {err:?}", e.message())),
            Ok(_) => Err(format!("expected error {err:?}, got contours")),
        }
    } else {
        let family = case
            .get("family")
            .and_then(|v| v.as_str())
            .ok_or("missing family")?;
        let bold = case.get("bold").and_then(|v| v.as_bool()).unwrap_or(false);
        let italic = case.get("italic").and_then(|v| v.as_bool()).unwrap_or(false);
        let font = fs::read(bundled_font_path(family, bold, italic)).map_err(|e| e.to_string())?;
        let opts = TextOpts {
            spacing: case.get("spacing").and_then(|v| v.as_f64()).unwrap_or(0.0),
            align: case
                .get("align")
                .and_then(|v| v.as_str())
                .unwrap_or("left")
                .to_string(),
            line_gap: case
                .get("line_gap")
                .and_then(|v| v.as_f64())
                .unwrap_or(1.2),
        };
        let size = case.get("size").and_then(|v| v.as_f64()).unwrap_or(10.0);
        let text = case.get("text").and_then(|v| v.as_str()).unwrap_or("");

        let contours = text_contours(&font, text, size, &opts).map_err(|e| e.to_string())?;
        let extent = text_extent(&font, text, size, &opts).map_err(|e| e.to_string())?;

        contours_match(&contours, &exp["contours"])?;
        let ew = exp["extent"][0].as_f64().unwrap();
        let eh = exp["extent"][1].as_f64().unwrap();
        if !near(extent[0], ew) || !near(extent[1], eh) {
            return Err(format!(
                "extent [{}, {}] != [{ew}, {eh}]",
                extent[0], extent[1]
            ));
        }
        Ok(())
    }
}

#[test]
fn text_matches_golden() {
    let golden_path = tests_dir().join("parity/text_golden.json");
    let golden: Value =
        serde_json::from_str(&fs::read_to_string(&golden_path).expect("text_golden.json")).unwrap();
    let cases_dir = tests_dir().join("parity/text");
    let obj = golden.as_object().expect("golden object");
    assert!(!obj.is_empty(), "text_golden.json has no cases");

    // Every case file must have a golden key (and vice versa is checked by running keys).
    let mut case_files: Vec<String> = fs::read_dir(&cases_dir)
        .expect("parity/text")
        .flatten()
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) == Some("json") {
                p.file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string())
            } else {
                None
            }
        })
        .collect();
    case_files.sort();
    let mut missing_keys = Vec::new();
    for name in &case_files {
        if !obj.contains_key(name) {
            missing_keys.push(name.clone());
        }
    }
    assert!(
        missing_keys.is_empty(),
        "case file(s) have no golden key: {}",
        missing_keys.join(", ")
    );

    let mut names: Vec<_> = obj.keys().cloned().collect();
    names.sort();
    let mut failures = Vec::new();
    for name in &names {
        let path = cases_dir.join(format!("{name}.json"));
        if let Err(e) = run_case(&path, &golden) {
            failures.push(format!("{name}: {e}"));
        }
    }
    assert!(
        failures.is_empty(),
        "text golden mismatches:\n{}",
        failures.join("\n")
    );
}

fn sample_font_bytes() -> Vec<u8> {
    fs::read(bundled_font_path("DejaVu Sans", false, false)).expect("DejaVuSans.ttf")
}

#[test]
fn garbage_font_bytes_error() {
    let opts = TextOpts::default();
    let err = text_contours(b"not a font", "Hi", 10.0, &opts).unwrap_err();
    assert_eq!(err.message(), "Could not read this font.");
}

#[test]
fn size_zero_errors() {
    let font = sample_font_bytes();
    let opts = TextOpts::default();
    let err = text_contours(&font, "Hi", 0.0, &opts).unwrap_err();
    assert_eq!(err.message(), "The text size must be more than 0.");
}

#[test]
fn size_negative_errors() {
    let font = sample_font_bytes();
    let opts = TextOpts::default();
    let err = text_contours(&font, "Hi", -5.0, &opts).unwrap_err();
    assert_eq!(err.message(), "The text size must be more than 0.");
}

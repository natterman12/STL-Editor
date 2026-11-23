//! `xtask licences [--check]`: the page of third-party licence texts.
//!
//! The app is one file, so the licence texts of what it is built from travel inside it: the page
//! `crates/stl-app/assets/licences.html` is built into the desktop app (Help > About >
//! Third-party licences) and copied beside the web edition. `cargo-about` collects the crates'
//! licence texts (settings `about.toml`, layout `about.hbs`); the two fonts in
//! `crates/stl-app/assets/fonts` are not crates, so their licence files are added here.
//!
//! Run it after adding or updating a crate. `--check` writes nothing and fails when the page in
//! the repository is not what would be written now (CI's "Dependencies" job).

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Where the fonts' part goes in `about.hbs`.
const FONTS_MARK: &str = "<!--FONTS-->";

/// The fonts built into the app that are not crates: what it is, and its licence file in
/// `crates/stl-app/assets/fonts`.
const FONTS: [(&str, &str); 2] = [
    ("Selawik, the font of the interface", "Selawik-OFL.txt"),
    (
        "Noto Emoji, a small piece of it for the heart and warning marks",
        "Symbols-OFL.txt",
    ),
];

pub fn run(args: impl Iterator<Item = String>) -> Result<(), String> {
    let check = args.into_iter().any(|a| a == "--check");
    let rust = rust_dir();
    let page = rust.join("crates/stl-app/assets/licences.html");
    let new = build(&rust)?;
    if check {
        let old = fs::read_to_string(&page).map_err(|e| format!("no {}: {e}", page.display()))?;
        // a Windows checkout may have turned the line ends into CRLF
        if unix_lines(&old) != new {
            return Err(format!(
                "{} is out of date: run `cargo run -p xtask -- licences` from rust/ and commit it",
                page.display()
            ));
        }
        println!("licences: up to date ({} KB)", new.len() / 1024);
        return Ok(());
    }
    fs::write(&page, &new).map_err(|e| format!("could not write {}: {e}", page.display()))?;
    println!("licences: wrote {} ({} KB)", page.display(), new.len() / 1024);
    Ok(())
}

fn rust_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The whole page: the crates' part from `cargo-about`, then the fonts.
fn build(rust: &Path) -> Result<String, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    // into a file: cargo-about refuses to write to a pipe under PowerShell
    let tmp = std::env::temp_dir().join(format!("stl-editor-licences-{}.html", std::process::id()));
    let out = Command::new(&cargo)
        .current_dir(rust)
        .arg("about")
        .arg("generate")
        .arg("--output-file")
        .arg(&tmp)
        .args([
            "--manifest-path",
            "crates/stl-app/Cargo.toml",
            "--config",
            "about.toml",
            "--locked",
            "--fail",
            "about.hbs",
        ])
        .output()
        .map_err(|e| format!("could not run cargo: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo about failed (install it once with `cargo install cargo-about --locked --features cli`):\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let crates = fs::read_to_string(&tmp).map_err(|e| format!("cargo about wrote no page: {e}"))?;
    let _ = fs::remove_file(&tmp);
    let mut fonts = Vec::new();
    for (what, file) in FONTS {
        let path = rust.join("crates/stl-app/assets/fonts").join(file);
        let text = fs::read_to_string(&path).map_err(|e| format!("no {}: {e}", path.display()))?;
        fonts.push((what, text));
    }
    add_fonts(&unix_lines(&crates), &fonts)
}

/// Put the fonts' licence texts where `about.hbs` marks the place.
fn add_fonts(page: &str, fonts: &[(&str, String)]) -> Result<String, String> {
    if !page.contains(FONTS_MARK) {
        return Err(format!("about.hbs has lost its {FONTS_MARK} line"));
    }
    let mut part = String::from("<h2 id=\"fonts\">The fonts</h2>\n");
    for (what, text) in fonts {
        part.push_str(&format!(
            "<h3>{}</h3>\n<pre>{}</pre>\n",
            escape(what),
            escape(unix_lines(text).trim_end())
        ));
    }
    Ok(page.replacen(FONTS_MARK, part.trim_end(), 1))
}

/// Text as it may stand inside HTML.
fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn unix_lines(s: &str) -> String {
    s.replace("\r\n", "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fonts_go_where_the_page_marks_and_their_text_is_escaped() {
        let page = "<main>\ncrates\n<!--FONTS-->\n</main>\n";
        let fonts = [("A & B", "Copyright <someone>\r\nline two\r\n".to_owned())];
        let out = add_fonts(page, &fonts).unwrap();
        assert_eq!(
            out,
            "<main>\ncrates\n<h2 id=\"fonts\">The fonts</h2>\n<h3>A &amp; B</h3>\n<pre>Copyright &lt;someone&gt;\nline two</pre>\n</main>\n"
        );
    }

    #[test]
    fn a_page_without_the_mark_is_refused() {
        assert!(add_fonts("<main></main>", &[]).is_err());
    }

    #[test]
    fn the_font_licence_files_are_there() {
        for (_, file) in FONTS {
            let path = rust_dir().join("crates/stl-app/assets/fonts").join(file);
            assert!(path.is_file(), "{}", path.display());
        }
    }
}

//! Font discovery: ports `meshtools.font_dirs`, `font_files`, `list_fonts`,
//! `default_font` and `_font_path`. Native-only (module gated in `text/mod.rs`).

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use ttf_parser::{name_id, Face, PlatformId};

use crate::error::SketchError;

/// One installed outline font style.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FontInfo {
    pub family: String,
    pub style: String,
    pub path: PathBuf,
}

fn env_nonempty(key: &str) -> Option<String> {
    std::env::var(key).ok().filter(|s| !s.is_empty())
}

/// Folders to search for TrueType / OpenType fonts (same order as 1.x).
pub fn font_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(win) = env_nonempty("WINDIR") {
        dirs.push(PathBuf::from(win).join("Fonts"));
    }
    if let Some(la) = env_nonempty("LOCALAPPDATA") {
        dirs.push(PathBuf::from(la).join("Microsoft").join("Windows").join("Fonts"));
    }
    let home = dirs_next_home();
    dirs.push(PathBuf::from("/usr/share/fonts"));
    dirs.push(PathBuf::from("/usr/local/share/fonts"));
    dirs.push(home.join(".fonts"));
    let xdg = env_nonempty("XDG_DATA_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| home.join(".local").join("share"));
    dirs.push(xdg.join("fonts"));
    dirs.push(home.join(".local").join("share").join("fonts"));
    dirs.push(PathBuf::from("/System/Library/Fonts"));
    dirs.push(PathBuf::from("/Library/Fonts"));
    dirs.push(home.join("Library").join("Fonts"));

    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for d in dirs {
        if d.is_dir() {
            let key = d.to_string_lossy().to_string();
            if seen.insert(key) {
                out.push(d);
            }
        }
    }
    out
}

/// Like Python `os.path.expanduser("~")`: `USERPROFILE` on Windows, `HOME` on POSIX.
fn dirs_next_home() -> PathBuf {
    #[cfg(windows)]
    {
        env_nonempty("USERPROFILE")
            .or_else(|| env_nonempty("HOME"))
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }
    #[cfg(not(windows))]
    {
        env_nonempty("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."))
    }
}

/// Every `.ttf` / `.otf` file under these folders (recursive), like `font_files`.
pub fn font_files(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for d in dirs {
        collect_font_files(d, &mut out);
    }
    out
}

fn collect_font_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = match fs::read_dir(dir) {
        Ok(e) => e.flatten().collect(),
        Err(_) => return,
    };
    // Deterministic order close to a sorted walk (Python glob order is FS-dependent).
    entries.sort_by_key(|e| e.file_name());
    for entry in entries {
        let name = entry.file_name();
        let name_lossy = name.to_string_lossy();
        // Python glob skips dot-entries by default (`include_hidden=False`).
        if name_lossy.starts_with('.') {
            continue;
        }
        let path = entry.path();
        let ft = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ft.is_symlink() {
            // Do not follow symlinked directories; still accept symlinked font files.
            match fs::metadata(&path) {
                Ok(m) if m.is_dir() => continue,
                Ok(m) if m.is_file() => {
                    if is_font_file(&path) {
                        out.push(path);
                    }
                }
                _ => {}
            }
            continue;
        }
        if ft.is_dir() {
            collect_font_files(&path, out);
            continue;
        }
        if ft.is_file() && is_font_file(&path) {
            out.push(path);
        }
    }
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| {
            let e = e.to_ascii_lowercase();
            e == "ttf" || e == "otf"
        })
        .unwrap_or(false)
}

static LIST_FONTS_CACHE: OnceLock<Vec<FontInfo>> = OnceLock::new();

/// Outline fonts on this computer: one [`FontInfo`] per family + style.
/// Cached like 1.x `@lru_cache(maxsize=1)` on `list_fonts`.
pub fn list_fonts() -> Vec<FontInfo> {
    LIST_FONTS_CACHE
        .get_or_init(|| list_fonts_in(&font_dirs()))
        .clone()
}

/// Like [`list_fonts`] but only looking in `dirs` (used by tests / callers that pin fonts).
pub fn list_fonts_in(dirs: &[PathBuf]) -> Vec<FontInfo> {
    // family -> styles in file order (first wins, matching 1.x setdefault)
    let mut fams: HashMap<String, Vec<(String, PathBuf)>> = HashMap::new();
    for path in font_files(dirs) {
        let data = match fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let face = match Face::parse(&data, 0) {
            Ok(f) => f,
            Err(_) => continue,
        };
        if face.tables().glyf.is_none() && face.tables().cff.is_none() {
            continue;
        }
        let fam = best_name(
            &face,
            &[
                name_id::WWS_FAMILY,
                name_id::TYPOGRAPHIC_FAMILY,
                name_id::FAMILY,
            ],
        )
        .unwrap_or_else(|| {
            path.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Font".into())
        });
        let sub = best_name(
            &face,
            &[
                name_id::WWS_SUBFAMILY,
                name_id::TYPOGRAPHIC_SUBFAMILY,
                name_id::SUBFAMILY,
            ],
        )
        .unwrap_or_else(|| "Regular".into());
        let styles = fams.entry(fam).or_default();
        if !styles.iter().any(|(s, _)| s == &sub) {
            styles.push((sub, path));
        }
    }
    let mut families: Vec<_> = fams.into_iter().collect();
    families.sort_by(|a, b| a.0.to_ascii_lowercase().cmp(&b.0.to_ascii_lowercase()));
    let mut out = Vec::new();
    for (family, styles) in families {
        for (style, path) in styles {
            out.push(FontInfo {
                family: family.clone(),
                style,
                path,
            });
        }
    }
    out
}

/// fontTools `name.getFirstDebugName` / `getDebugName`: prefer Windows English
/// `(platform=3, encoding=1, lang=0x409)`, then Mac English, then any.
fn best_name(face: &Face<'_>, ids: &[u16]) -> Option<String> {
    for &id in ids {
        if let Some(s) = debug_name(face, id) {
            return Some(s);
        }
    }
    None
}

fn debug_name(face: &Face<'_>, id: u16) -> Option<String> {
    let mut english: Option<String> = None;
    let mut mac_english: Option<String> = None;
    let mut some: Option<String> = None;
    for name in face.names() {
        if name.name_id != id {
            continue;
        }
        let Some(s) = name.to_string() else {
            continue;
        };
        if s.is_empty() {
            continue;
        }
        some = Some(s.clone());
        // Prefer the Windows English name record (3, 1, 0x409) as fontTools does.
        if name.platform_id == PlatformId::Windows
            && name.encoding_id == 1
            && name.language_id == 0x409
        {
            return Some(s);
        }
        if english.is_none()
            && name.platform_id == PlatformId::Windows
            && name.language_id == 0x409
        {
            english = Some(s.clone());
        }
        if mac_english.is_none()
            && name.platform_id == PlatformId::Macintosh
            && name.language_id == 0
        {
            mac_english = Some(s);
        }
    }
    english.or(mac_english).or(some)
}

/// Preferred family path (Regular / Book / …), same preference list as 1.x `default_font`.
pub fn default_font() -> Option<PathBuf> {
    default_font_in(&list_fonts())
}

fn default_font_in(fonts: &[FontInfo]) -> Option<PathBuf> {
    for want in [
        "Arial",
        "Segoe UI",
        "DejaVu Sans",
        "Liberation Sans",
        "Helvetica",
        "Noto Sans",
    ] {
        if fonts.iter().any(|f| f.family == want) {
            return Some(pick_style_from(fonts, want, false, false));
        }
    }
    // Fallback: first family in lower-case list order (`fonts.first()` after sort).
    let first_fam = fonts.first()?.family.as_str();
    Some(pick_style_from(fonts, first_fam, false, false))
}

/// Resolve a family + bold/italic to a font file path (`meshtools._font_path`).
pub fn font_path(family: Option<&str>, bold: bool, italic: bool) -> Result<PathBuf, SketchError> {
    font_path_among(&list_fonts(), family, bold, italic)
}

pub fn font_path_among(
    fonts: &[FontInfo],
    family: Option<&str>,
    bold: bool,
    italic: bool,
) -> Result<PathBuf, SketchError> {
    let mut fam = family.filter(|s| !s.is_empty()).map(|s| s.to_string());
    if fam
        .as_deref()
        .map(|s| !fonts.iter().any(|f| f.family == s))
        .unwrap_or(true)
    {
        fam = default_font_in(fonts).and_then(|p| {
            fonts
                .iter()
                .find(|f| f.path == p)
                .map(|f| f.family.clone())
        });
        if fam.is_none() {
            return Err(SketchError::new(
                "No outline fonts were found on this computer.",
            ));
        }
    }
    let fam = fam.unwrap();
    if !fonts.iter().any(|f| f.family == fam) {
        return Err(SketchError::new(
            "No outline fonts were found on this computer.",
        ));
    }
    Ok(pick_style_from(fonts, &fam, bold, italic))
}

fn pick_style_from(fonts: &[FontInfo], family: &str, bold: bool, italic: bool) -> PathBuf {
    let styles: Vec<&FontInfo> = fonts.iter().filter(|f| f.family == family).collect();
    let mut want = String::new();
    if bold {
        want.push_str("Bold ");
    }
    if italic {
        want.push_str("Italic");
    }
    let want = want.trim();
    let want = if want.is_empty() { "Regular" } else { want };
    let oblique = want.replace("Italic", "Oblique");
    for k in [
        want,
        oblique.as_str(),
        "Regular",
        "Normal",
        "Book",
        "Roman",
    ] {
        if let Some(f) = styles.iter().find(|f| f.style == k) {
            return f.path.clone();
        }
    }
    // First style in file order (not alphabetical).
    styles
        .first()
        .expect("non-empty styles")
        .path
        .clone()
}

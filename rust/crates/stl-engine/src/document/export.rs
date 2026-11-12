//! File export (1.x `Document.export` and the `r_export` base-name rule).

use std::borrow::Cow;
use std::sync::Arc;

use stl_io::{ExportBody, ExportError, ExportFormat, ExportOptions};
use stl_kernel::MeshArrays;

use super::Document;
use crate::FeatureError;

/// The last path component, for `/` and `\` alike.
fn file_name(path: &str) -> &str {
    &path[path.rfind(['/', '\\']).map_or(0, |i| i + 1)..]
}

/// Python's `os.path.splitext(name)[0]`: the last extension goes; a leading dot is not one.
fn strip_ext(name: &str) -> &str {
    let lead = name.len() - name.trim_start_matches('.').len();
    match name[lead..].rfind('.') {
        Some(i) => &name[..lead + i],
        None => name,
    }
}

/// A trailing `.stlx`, whatever its case, is removed.
fn strip_stlx(s: &str) -> &str {
    match s.len().checked_sub(5) {
        Some(i) if s.is_char_boundary(i) && s[i..].eq_ignore_ascii_case(".stlx") => &s[..i],
        _ => s,
    }
}

impl Document {
    /// The ids that get exported: `ids` if given and not empty, else the bodies not hidden, else
    /// all bodies. Errors when there is none.
    pub fn export_bodies_for(&self, ids: Option<&[String]>) -> Result<Vec<String>, FeatureError> {
        let ids: Vec<String> = match ids.filter(|v| !v.is_empty()) {
            Some(v) => v.to_vec(),
            None => {
                let hidden = self.hidden();
                let shown: Vec<String> =
                    self.state.bodies.keys().filter(|b| !hidden.contains(*b)).cloned().collect();
                if shown.is_empty() { self.state.bodies.keys().cloned().collect() } else { shown }
            }
        };
        if ids.is_empty() {
            return Err(FeatureError("There is nothing to export.".into()));
        }
        Ok(ids)
    }

    /// The file name (without extension) the Save dialog offers, and the STEP title.
    pub fn export_base_name(&self, ids: &[String], project_path: Option<&str>) -> String {
        let base = if ids.len() == 1 {
            self.body_name(&ids[0])
        } else if let Some(p) = project_path.filter(|p| !p.is_empty()) {
            strip_ext(file_name(p)).to_string()
        } else if let Some(t) = self.title.as_deref().filter(|t| !t.is_empty()) {
            t.to_string()
        } else {
            "model".to_string()
        };
        strip_stlx(&base).to_string()
    }

    /// The file's bytes and how many bodies went into it.
    pub fn export_bytes(
        &self,
        format: ExportFormat,
        ids: Option<&[String]>,
        opts: &ExportOptions,
    ) -> Result<(Vec<u8>, usize), FeatureError> {
        let ids = self.export_bodies_for(ids)?;
        let mut owned: Vec<(String, Cow<MeshArrays>)> = Vec::new();
        for id in &ids {
            let b = self.state.body(id)?;
            let mesh = match (&b.solid, &b.mesh) {
                (Some(s), _) => Cow::Owned(s.to_mesh()),
                (None, Some(m)) => Cow::Borrowed(Arc::as_ref(m)),
                (None, None) => Cow::Owned(MeshArrays { positions: Vec::new(), triangles: Vec::new() }),
            };
            owned.push((self.body_name(id), mesh));
        }
        let bodies: Vec<ExportBody> =
            owned.iter().map(|(n, m)| ExportBody { name: n.clone(), mesh: m }).collect();
        let bytes = stl_io::export_bodies(&bodies, format, opts).map_err(|e| match e {
            ExportError::Geometry(m) | ExportError::Io(m) => FeatureError(m),
            ExportError::Unsupported => FeatureError(format!("Can't export .{} files.", format.ext())),
        })?;
        Ok((bytes, bodies.len()))
    }

    /// Writes the file. A path with no extension gets the format's; otherwise the extension picks
    /// the format. Returns the path written and the number of bodies.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn export(
        &self,
        path: &std::path::Path,
        format: ExportFormat,
        ids: Option<&[String]>,
        project_path: Option<&str>,
    ) -> Result<(std::path::PathBuf, usize), FeatureError> {
        let (path, format) = match path.extension() {
            None => {
                let mut s = path.as_os_str().to_os_string();
                s.push(".");
                s.push(format.ext());
                (std::path::PathBuf::from(s), format)
            }
            Some(e) => {
                let e = e.to_string_lossy();
                match ExportFormat::from_ext(&e) {
                    Some(f) => (path.to_path_buf(), f),
                    None => return Err(FeatureError(format!("Can't export .{e} files."))),
                }
            }
        };
        let ids = self.export_bodies_for(ids)?;
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
        let opts = ExportOptions {
            uuid_seed: now.as_nanos() as u64,
            step_title: self.export_base_name(&ids, project_path),
            step_date: iso_from_unix_secs(now.as_secs()),
        };
        let (bytes, n) = self.export_bytes(format, Some(&ids), &opts)?;
        std::fs::write(&path, bytes).map_err(|e| FeatureError(format!("Couldn't write {}: {e}", path.display())))?;
        Ok((path, n))
    }
}

/// `YYYY-MM-DDTHH:MM:SS` (UTC) for seconds since 1970.
pub fn iso_from_unix_secs(secs: u64) -> String {
    let days = (secs / 86_400) as i64;
    let rem = secs % 86_400;
    // civil-from-days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// The current UTC time as `YYYY-MM-DDTHH:MM:SS`. On wasm32 there is no clock here (the browser
/// supplies the time), so it is the epoch.
pub fn iso_utc_now() -> String {
    #[cfg(not(target_arch = "wasm32"))]
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    #[cfg(target_arch = "wasm32")]
    let secs = 0;
    iso_from_unix_secs(secs)
}

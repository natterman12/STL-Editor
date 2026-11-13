mod bench;
mod pure;
mod uibench;
mod web;

use std::process::{Command, ExitCode};

const TARGETS: [&str; 4] = [
    "x86_64-pc-windows-gnu",
    "x86_64-unknown-linux-gnu",
    "aarch64-apple-darwin",
    "wasm32-unknown-unknown",
];

fn pure_rust() -> Result<bool, String> {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let mut ok = true;
    for target in TARGETS {
        let out = Command::new(&cargo)
            .args(["metadata", "--format-version", "1", "--filter-platform", target])
            .output()
            .map_err(|e| format!("could not run cargo: {e}"))?;
        if !out.status.success() {
            return Err(format!("cargo metadata failed for {target}: {}", String::from_utf8_lossy(&out.stderr)));
        }
        let meta: serde_json::Value = serde_json::from_slice(&out.stdout).map_err(|e| e.to_string())?;
        let allow: &[&str] = if target == pure::LINUX_ALLOW_TARGET { &pure::LINUX_ALLOW } else { &[] };
        if !allow.is_empty() {
            println!("pure-rust: {target}: allowed pkg-config/system-deps for {} ({})", allow.join(", "), pure::LINUX_ALLOW_REASON);
        }
        for name in pure::forbidden_except(&meta, allow) {
            println!("pure-rust: {target}: {name}");
            ok = false;
        }
    }
    if ok {
        println!("pure-rust: ok ({} targets)", TARGETS.len());
    }
    Ok(ok)
}

fn main() -> ExitCode {
    match std::env::args().nth(1).as_deref() {
        Some("pure-rust") => match pure_rust() {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => {
                eprintln!("pure-rust: {e}");
                ExitCode::FAILURE
            }
        },
        Some("bench") => match bench::run() {
            Ok(true) => ExitCode::SUCCESS,
            Ok(false) => ExitCode::FAILURE,
            Err(e) => {
                eprintln!("bench: {e}");
                ExitCode::FAILURE
            }
        },
        Some("web") => match web::run(std::env::args().skip(2)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("web: {e}");
                ExitCode::FAILURE
            }
        },
        Some("uibench") => match uibench::run(std::env::args().skip(2)) {
            Ok(()) => ExitCode::SUCCESS,
            Err(e) => {
                eprintln!("uibench: {e}");
                ExitCode::FAILURE
            }
        },
        _ => {
            eprintln!("usage: cargo run -p xtask -- pure-rust | cargo run --release -p xtask -- bench | cargo run --release -p xtask -- uibench [--quick] | cargo run -p xtask -- web [--serve | --check [--webgl2]]");
            ExitCode::FAILURE
        }
    }
}

use std::path::{Path, PathBuf};
use std::process::Command;

fn run_git(repo_root: &Path, args: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8(output.stdout).ok()?;
    Some(text.trim().to_string())
}

fn sanitize_tag(tag: Option<String>) -> String {
    match tag {
        Some(value) if value.starts_with('v') => value[1..].to_string(),
        Some(value) if !value.is_empty() => value,
        _ => "0.0.0".to_string(),
    }
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_root = manifest_dir.join("..").canonicalize().unwrap_or(manifest_dir.clone());

    let version_tag = sanitize_tag(run_git(&repo_root, &["describe", "--tags", "--abbrev=0"]));
    let build_number = run_git(&repo_root, &["rev-list", "--count", "HEAD"]).unwrap_or_else(|| "0".to_string());
    let git_sha = run_git(&repo_root, &["rev-parse", "--short=8", "HEAD"]).unwrap_or_else(|| "nogit".to_string());
    let dirty = run_git(&repo_root, &["status", "--porcelain"]).map(|s| !s.is_empty()).unwrap_or(false);

    let mut version_text = format!("{}+build.{}.{}", version_tag, build_number, git_sha);
    if dirty {
        version_text.push_str(".dirty");
    }

    println!("cargo:rustc-env=FLASHBANG_VERSION_TAG={}", version_tag);
    println!("cargo:rustc-env=FLASHBANG_BUILD_NUMBER={}", build_number);
    println!("cargo:rustc-env=FLASHBANG_GIT_SHA={}", git_sha);
    println!("cargo:rustc-env=FLASHBANG_GIT_DIRTY={}", if dirty { "1" } else { "0" });
    println!("cargo:rustc-env=FLASHBANG_VERSION_TEXT={}", version_text);

    println!("cargo:rerun-if-changed={}", repo_root.join(".git/HEAD").display());
    println!("cargo:rerun-if-changed={}", repo_root.join(".git/index").display());
    println!("cargo:rerun-if-changed={}", repo_root.join(".git/packed-refs").display());
    println!("cargo:rerun-if-changed={}", repo_root.join(".git/refs/heads").display());
    println!("cargo:rerun-if-changed={}", repo_root.join(".git/refs/tags").display());
}

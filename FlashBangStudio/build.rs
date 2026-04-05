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

fn run_git_bytes(repo_root: &Path, args: &[&str]) -> Option<Vec<u8>> {
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    Some(output.stdout)
}

fn strip_leading_v(value: &str) -> &str {
    value.strip_prefix('v').unwrap_or(value)
}

fn parse_semver_triplet(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next()?.parse().ok()?;
    let patch = parts.next()?.parse().ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((major, minor, patch))
}

fn read_protocol_version(repo_root: &Path) -> String {
    let path = repo_root.join("protocol").join("VERSION");
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn extract_studio_version_from_tag(raw_tag: Option<String>) -> String {
    let Some(tag) = raw_tag else {
        return "unknown".to_string();
    };

    let tag = tag.trim();
    if tag.is_empty() {
        return "unknown".to_string();
    }

    // Triple-component release tag format: Fx.y.z-Px.y.z-Sx.y.z
    if let Some(studio) = tag.split("-S").nth(1) {
        let studio = strip_leading_v(studio.trim());
        if parse_semver_triplet(studio).is_some() {
            return studio.to_string();
        }
    }

    // Legacy/simple tags like v0.4.3
    let legacy = strip_leading_v(tag);
    if parse_semver_triplet(legacy).is_some() {
        return legacy.to_string();
    }

    "unknown".to_string()
}

fn build_datetime_yyyymmdd_hhmm() -> String {
    let output = Command::new("date")
        .arg("+%Y%m%d-%H%M")
        .output();

    if let Ok(out) = output {
        if out.status.success() {
            if let Ok(text) = String::from_utf8(out.stdout) {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    return trimmed.to_string();
                }
            }
        }
    }

    "unknown".to_string()
}

fn fnv1a64_update(mut hash: u64, bytes: &[u8]) -> u64 {
    const FNV_PRIME: u64 = 1099511628211;
    for b in bytes {
        hash ^= u64::from(*b);
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

fn collect_non_ignored_git_files(repo_root: &Path) -> Vec<String> {
    let Some(raw) = run_git_bytes(
        repo_root,
        &["ls-files", "-z", "--cached", "--others", "--exclude-standard"],
    ) else {
        return Vec::new();
    };

    let mut files = raw
        .split(|b| *b == 0)
        .filter(|part| !part.is_empty())
        .filter_map(|part| String::from_utf8(part.to_vec()).ok())
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn workspace_content_hash(repo_root: &Path, files: &[String]) -> String {
    // FNV-1a over path + content for deterministic, cheap build identity.
    let mut hash: u64 = 14695981039346656037;
    for rel in files {
        hash = fnv1a64_update(hash, rel.as_bytes());
        hash = fnv1a64_update(hash, &[0]);

        let abs = repo_root.join(rel);
        if let Ok(content) = std::fs::read(&abs) {
            hash = fnv1a64_update(hash, &content);
        }

        hash = fnv1a64_update(hash, &[0xff]);
    }
    format!("{:016x}", hash)
}

fn main() {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo_root = manifest_dir.join("..").canonicalize().unwrap_or(manifest_dir.clone());
    let local_version = std::env::var("CARGO_PKG_VERSION").unwrap_or_else(|_| "unknown".to_string());
    let non_ignored_files = collect_non_ignored_git_files(&repo_root);

    let based_on_release = extract_studio_version_from_tag(run_git(
        &repo_root,
        &["describe", "--tags", "--abbrev=0", "--first-parent"],
    ));
    let build_number = run_git(&repo_root, &["rev-list", "--count", "HEAD"]).unwrap_or_else(|| "0".to_string());
    let git_sha = run_git(&repo_root, &["rev-parse", "--short=8", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let dirty = run_git(&repo_root, &["status", "--porcelain"]).map(|s| !s.is_empty()).unwrap_or(false);
    let build_datetime = build_datetime_yyyymmdd_hhmm();
    let protocol_version = read_protocol_version(&repo_root);
    let workspace_hash = workspace_content_hash(&repo_root, &non_ignored_files);

    // Guard: local studio version must not go below the release baseline if both are semver.
    if let (Some(local), Some(release)) = (
        parse_semver_triplet(&local_version),
        parse_semver_triplet(&based_on_release),
    ) {
        if local < release {
            panic!(
                "Studio version guard failed: local CARGO_PKG_VERSION ({}) must be >= based-on-release tag ({})",
                local_version, based_on_release
            );
        }
    }

    let mut version_text = format!(
        "{}+build.{}.{}.{}",
        local_version, build_datetime, workspace_hash, git_sha
    );
    if dirty {
        version_text.push_str(".dirty");
    }

    println!("cargo:rustc-env=FLASHBANG_BASED_ON_RELEASE={}", based_on_release);
    println!("cargo:rustc-env=FLASHBANG_BUILD_NUMBER={}", build_number);
    println!("cargo:rustc-env=FLASHBANG_GIT_SHA={}", git_sha);
    println!("cargo:rustc-env=FLASHBANG_GIT_DIRTY={}", if dirty { "1" } else { "0" });
    println!("cargo:rustc-env=FLASHBANG_BUILD_DATETIME={}", build_datetime);
    println!("cargo:rustc-env=FLASHBANG_PROTOCOL_VERSION={}", protocol_version);
    println!("cargo:rustc-env=FLASHBANG_WORKSPACE_HASH={}", workspace_hash);
    println!("cargo:rustc-env=FLASHBANG_VERSION_TEXT={}", version_text);

    // Recompute embedded metadata whenever Studio sources/config change.
    println!("cargo:rerun-if-changed={}", manifest_dir.join("src").display());
    println!("cargo:rerun-if-changed={}", manifest_dir.join("Cargo.toml").display());
    println!("cargo:rerun-if-changed={}", manifest_dir.join("build.rs").display());

    println!("cargo:rerun-if-changed={}", repo_root.join(".git/HEAD").display());
    println!("cargo:rerun-if-changed={}", repo_root.join(".git/index").display());
    println!("cargo:rerun-if-changed={}", repo_root.join(".git/packed-refs").display());
    println!("cargo:rerun-if-changed={}", repo_root.join(".git/refs/heads").display());
    println!("cargo:rerun-if-changed={}", repo_root.join(".git/refs/tags").display());
    println!("cargo:rerun-if-changed={}", repo_root.join("protocol/VERSION").display());

    for rel in non_ignored_files {
        println!("cargo:rerun-if-changed={}", repo_root.join(rel).display());
    }
}

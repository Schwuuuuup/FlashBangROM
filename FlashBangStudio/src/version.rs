pub fn based_on_release() -> &'static str {
    env!("FLASHBANG_BASED_ON_RELEASE")
}

pub fn git_sha() -> &'static str {
    env!("FLASHBANG_GIT_SHA")
}

pub fn is_dirty() -> bool {
    env!("FLASHBANG_GIT_DIRTY") == "1"
}

pub fn version_text() -> &'static str {
    env!("FLASHBANG_VERSION_TEXT")
}

pub fn build_datetime() -> &'static str {
    env!("FLASHBANG_BUILD_DATETIME")
}

pub fn package_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

pub fn supported_protocol_version() -> &'static str {
    "0.6.0"
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

pub fn is_protocol_compatible(remote_version: &str) -> bool {
    let minimum = supported_protocol_version();
    match (parse_semver_triplet(remote_version), parse_semver_triplet(minimum)) {
        (Some(remote), Some(min)) => remote >= min,
        _ => remote_version == minimum,
    }
}

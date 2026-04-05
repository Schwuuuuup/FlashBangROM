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
    env!("FLASHBANG_PROTOCOL_VERSION")
}

pub fn version_tag() -> &'static str {
    env!("FLASHBANG_VERSION_TAG")
}

pub fn build_number() -> &'static str {
    env!("FLASHBANG_BUILD_NUMBER")
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

#include "version_info.h"

#include "generated_build_info.h"

const char* firmwareVersionTag() {
  return FB_VERSION_TAG;
}

const char* firmwareGitSha() {
  return FB_GIT_SHA;
}

const char* firmwareVersionText() {
  return FB_VERSION_TEXT;
}

uint32_t firmwareBuildNumber() {
  return static_cast<uint32_t>(FB_BUILD_NUMBER);
}

bool firmwareIsDirty() {
  return FB_GIT_DIRTY;
}

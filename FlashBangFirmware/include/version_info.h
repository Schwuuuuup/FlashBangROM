#pragma once

#include <Arduino.h>

const char* firmwareVersionTag();
const char* firmwareProtocolVersion();
const char* firmwareGitSha();
const char* firmwareVersionText();
uint32_t firmwareBuildNumber();
bool firmwareIsDirty();

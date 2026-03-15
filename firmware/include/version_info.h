#pragma once

#include <Arduino.h>

const char* firmwareVersionTag();
const char* firmwareGitSha();
const char* firmwareVersionText();
uint32_t firmwareBuildNumber();
bool firmwareIsDirty();

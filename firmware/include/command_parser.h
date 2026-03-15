#pragma once

#include <Arduino.h>

#include "device_types.h"

bool parseHex32(const String& text, uint32_t& out);
bool parseDec32(const String& text, uint32_t& out);
bool parseLine(const String& line, CommandContext& ctx);
bool validateRange(uint32_t addr, uint32_t len);

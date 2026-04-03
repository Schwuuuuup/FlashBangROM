#pragma once

#include <Arduino.h>

#include "device_types.h"

bool parseHex32(const String& text, uint32_t& out);
bool parseDec32(const String& text, uint32_t& out);
bool parseLine(const String& line, CommandContext& ctx);
bool validateRange(uint32_t addr, uint32_t len);

// Raw line access for commands that need the original (non-uppercased) text.
// Set by parseLine when the command needs the original line (SEQUENCE, PARAMETER, CustomSequence).
extern String g_rawLine;

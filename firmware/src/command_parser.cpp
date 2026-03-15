#include "command_parser.h"

#include <cstdlib>

#include "device_globals.h"

bool parseHex32(const String& text, uint32_t& out) {
  char* endPtr = nullptr;
  unsigned long parsed = strtoul(text.c_str(), &endPtr, 16);
  if (endPtr == nullptr || *endPtr != '\0') {
    return false;
  }
  out = static_cast<uint32_t>(parsed);
  return true;
}

bool parseDec32(const String& text, uint32_t& out) {
  char* endPtr = nullptr;
  unsigned long parsed = strtoul(text.c_str(), &endPtr, 10);
  if (endPtr == nullptr || *endPtr != '\0') {
    return false;
  }
  out = static_cast<uint32_t>(parsed);
  return true;
}

bool parseLine(const String& line, CommandContext& ctx) {
  if (line == "?") {
    ctx.cmd = CommandType::Help;
    return true;
  }
  if (line == "ID") {
    ctx.cmd = CommandType::Id;
    return true;
  }
  if (line == "CHIP_ERASE") {
    ctx.cmd = CommandType::ChipErase;
    return true;
  }

  int p1 = line.indexOf('|');
  if (p1 < 0) {
    return false;
  }
  String op = line.substring(0, p1);

  if (op == "READ") {
    int p2 = line.indexOf('|', p1 + 1);
    if (p2 < 0) {
      return false;
    }
    if (!parseHex32(line.substring(p1 + 1, p2), ctx.addr)) {
      return false;
    }
    if (!parseDec32(line.substring(p2 + 1), ctx.len)) {
      return false;
    }
    ctx.cmd = CommandType::Read;
    return true;
  }

  if (op == "PROGRAM_BYTE") {
    int p2 = line.indexOf('|', p1 + 1);
    uint32_t value = 0;
    if (p2 < 0) {
      return false;
    }
    if (!parseHex32(line.substring(p1 + 1, p2), ctx.addr)) {
      return false;
    }
    if (!parseHex32(line.substring(p2 + 1), value)) {
      return false;
    }
    ctx.value = static_cast<uint8_t>(value & 0xFF);
    ctx.cmd = CommandType::ProgramByte;
    return true;
  }

  if (op == "SECTOR_ERASE") {
    if (!parseHex32(line.substring(p1 + 1), ctx.addr)) {
      return false;
    }
    ctx.cmd = CommandType::SectorErase;
    return true;
  }

  if (op == "WRITE_STATUS") {
    int p2 = line.indexOf('|', p1 + 1);
    int p3 = line.indexOf('|', p2 + 1);
    uint32_t expected = 0;
    if (p2 < 0 || p3 < 0) {
      return false;
    }
    if (!parseHex32(line.substring(p1 + 1, p2), ctx.addr)) {
      return false;
    }
    if (!parseHex32(line.substring(p2 + 1, p3), expected)) {
      return false;
    }
    if (!parseDec32(line.substring(p3 + 1), ctx.timeoutMs)) {
      return false;
    }
    ctx.value = static_cast<uint8_t>(expected & 0xFF);
    ctx.cmd = CommandType::WriteStatus;
    return true;
  }

  return false;
}

bool validateRange(uint32_t addr, uint32_t len) {
  if (len == 0) {
    return false;
  }
  if (addr >= g_chipSizeBytes) {
    return false;
  }
  if (len > g_chipSizeBytes) {
    return false;
  }
  if (addr > (g_chipSizeBytes - len)) {
    return false;
  }
  return true;
}

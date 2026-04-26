#include "command_parser.h"

#include <cstdlib>

#include "device_globals.h"

String g_rawLine;

bool parseHex32(const String& text, uint32_t& out) {
  String normalized = text;
  normalized.trim();
  char* endPtr = nullptr;
  unsigned long parsed = strtoul(normalized.c_str(), &endPtr, 16);
  if (endPtr == nullptr || *endPtr != '\0') {
    return false;
  }
  out = static_cast<uint32_t>(parsed);
  return true;
}

bool parseDec32(const String& text, uint32_t& out) {
  String normalized = text;
  normalized.trim();
  char* endPtr = nullptr;
  unsigned long parsed = strtoul(normalized.c_str(), &endPtr, 10);
  if (endPtr == nullptr || *endPtr != '\0') {
    return false;
  }
  out = static_cast<uint32_t>(parsed);
  return true;
}

bool parseLine(const String& line, CommandContext& ctx) {
  String trimmed = line;
  trimmed.trim();

  if (trimmed.length() == 0) {
    return false;
  }

  // Save raw line for commands that need case-sensitive input
  g_rawLine = trimmed;

  // Special-char shorthand mode (0x20..0x3F subset used here).
  if (trimmed == "!") {
    ctx.cmd = CommandType::Flush;
    ctx.shortForm = true;
    return true;
  }
  if (trimmed.length() >= 1 && trimmed.charAt(0) == '>') {
    // >AAAAADD  (5 hex addr + 2 hex data)
    if (trimmed.length() != 8) {
      return false;
    }
    uint32_t value = 0;
    if (!parseHex32(trimmed.substring(1, 6), ctx.addr)) {
      return false;
    }
    if (!parseHex32(trimmed.substring(6, 8), value)) {
      return false;
    }
    ctx.value = static_cast<uint8_t>(value & 0xFF);
    ctx.cmd = CommandType::ProgramByte;
    ctx.shortForm = true;
    return true;
  }
  if (trimmed.length() >= 1 && trimmed.charAt(0) == '<') {
    // <AAAAALLLL (5 hex addr + 4 hex len)
    if (trimmed.length() != 10) {
      return false;
    }
    if (!parseHex32(trimmed.substring(1, 6), ctx.addr)) {
      return false;
    }
    if (!parseHex32(trimmed.substring(6, 10), ctx.len)) {
      return false;
    }
    if (ctx.len == 0) {
      return false;
    }
    ctx.cmd = CommandType::Read;
    ctx.shortForm = true;
    return true;
  }
  if (trimmed.length() >= 1 && trimmed.charAt(0) == ':') {
    // :AAAAALLLL<hex-bytes> (5 hex addr + 4 hex payload length in bytes + payload)
    if (trimmed.length() < 10) {
      return false;
    }
    if (!parseHex32(trimmed.substring(1, 6), ctx.addr)) {
      return false;
    }
    if (!parseHex32(trimmed.substring(6, 10), ctx.len)) {
      return false;
    }
    if (ctx.len == 0) {
      return false;
    }
    const uint32_t payloadHexLen = ctx.len * 2UL;
    if (trimmed.length() != static_cast<int>(10UL + payloadHexLen)) {
      return false;
    }
    ctx.cmd = CommandType::ProgramRange;
    ctx.shortForm = true;
    return true;
  }

  // Fast path for compact aliases: <alias>|...
  if (trimmed.length() >= 2 && trimmed.charAt(1) == '|') {
    char alias = trimmed.charAt(0);
    if (alias >= 'a' && alias <= 'z') {
      alias = static_cast<char>(alias - ('a' - 'A'));
    }

    if (alias == 'S') {
      ctx.cmd = CommandType::Sequence;
      return true;
    }
    if (alias == 'P') {
      ctx.cmd = CommandType::Parameter;
      return true;
    }

    int p2 = trimmed.indexOf('|', 2);
    if (alias == 'R') {
      if (p2 < 0) {
        return false;
      }
      String addrText = trimmed.substring(2, p2);
      String lenText = trimmed.substring(p2 + 1);
      if (!parseHex32(addrText, ctx.addr)) {
        return false;
      }
      if (!parseDec32(lenText, ctx.len)) {
        return false;
      }
      ctx.cmd = CommandType::Read;
      return true;
    }

    if (alias == 'W') {
      uint32_t value = 0;
      if (p2 < 0) {
        return false;
      }
      String addrText = trimmed.substring(2, p2);
      String valueText = trimmed.substring(p2 + 1);
      if (!parseHex32(addrText, ctx.addr)) {
        return false;
      }
      if (!parseHex32(valueText, value)) {
        return false;
      }
      ctx.value = static_cast<uint8_t>(value & 0xFF);
      ctx.cmd = CommandType::ProgramByte;
      return true;
    }

    if (alias == 'G') {
      if (p2 < 0) {
        return false;
      }
      String addrText = trimmed.substring(2, p2);
      if (!parseHex32(addrText, ctx.addr)) {
        return false;
      }
      ctx.cmd = CommandType::ProgramRange;
      return true;
    }

    if (alias == 'E') {
      String addrText = trimmed.substring(2);
      if (!parseHex32(addrText, ctx.addr)) {
        return false;
      }
      ctx.cmd = CommandType::SectorErase;
      return true;
    }

    if (alias == 'T') {
      int p3 = (p2 >= 0) ? trimmed.indexOf('|', p2 + 1) : -1;
      uint32_t expected = 0;
      if (p2 < 0 || p3 < 0) {
        return false;
      }
      String addrText = trimmed.substring(2, p2);
      String expectedText = trimmed.substring(p2 + 1, p3);
      String timeoutText = trimmed.substring(p3 + 1);
      if (!parseHex32(addrText, ctx.addr)) {
        return false;
      }
      if (!parseHex32(expectedText, expected)) {
        return false;
      }
      if (!parseDec32(timeoutText, ctx.timeoutMs)) {
        return false;
      }
      ctx.value = static_cast<uint8_t>(expected & 0xFF);
      ctx.cmd = CommandType::WriteStatus;
      return true;
    }
  }

  // Lowercase first char → custom sequence dispatch (bit 5 check)
  char first = trimmed.charAt(0);
  if (first >= 'a' && first <= 'z') {
    ctx.cmd = CommandType::CustomSequence;
    return true;
  }

  // Normalize to uppercase for built-in command matching
  String normalized = trimmed;
  normalized.toUpperCase();

  if (normalized == "?") {
    ctx.cmd = CommandType::Help;
    return true;
  }
  if (normalized == "HELP") {
    ctx.cmd = CommandType::Help;
    return true;
  }
  if (normalized == "HELLO" || normalized.startsWith("HELLO|")) {
    ctx.cmd = CommandType::Hello;
    return true;
  }
  if (normalized == "ID") {
    ctx.cmd = CommandType::Id;
    return true;
  }
  if (normalized == "CHIP_ERASE") {
    ctx.cmd = CommandType::ChipErase;
    return true;
  }
  if (normalized == "C") {
    ctx.cmd = CommandType::ChipErase;
    return true;
  }
  if (normalized == "INSPECT") {
    ctx.cmd = CommandType::Inspect;
    return true;
  }
  if (normalized == "DRIVER_RESET") {
    ctx.cmd = CommandType::DriverReset;
    return true;
  }
  if (normalized == "DATA_BUS_MONITOR_START") {
    ctx.cmd = CommandType::DataBusMonitorStart;
    return true;
  }
  if (normalized == "DATA_BUS_MONITOR_STOP") {
    ctx.cmd = CommandType::DataBusMonitorStop;
    return true;
  }

  if (normalized.startsWith("SET_A")) {
    if (normalized.length() != 10) {
      return false;
    }
    uint32_t addr = 0;
    if (!parseHex32(normalized.substring(5), addr)) {
      return false;
    }
    if (addr > 0x7FFFFUL) {
      return false;
    }
    ctx.addr = addr;
    ctx.cmd = CommandType::SetAddress;
    return true;
  }

  int p1 = normalized.indexOf('|');
  if (p1 < 0) {
    return false;
  }
  String op = normalized.substring(0, p1);
  op.trim();

  if (op == "SEQUENCE") {
    // SEQUENCE|name|script — need raw line for case-sensitive name + script
    ctx.cmd = CommandType::Sequence;
    return true;
  }
  if (op == "S") {
    ctx.cmd = CommandType::Sequence;
    return true;
  }

  if (op == "PARAMETER") {
    // PARAMETER|key|value — need raw line for case-sensitive key
    ctx.cmd = CommandType::Parameter;
    return true;
  }
  if (op == "P") {
    ctx.cmd = CommandType::Parameter;
    return true;
  }

  if (op == "READ" || op == "R") {
    int p2 = normalized.indexOf('|', p1 + 1);
    if (p2 < 0) {
      return false;
    }
    String addrText = normalized.substring(p1 + 1, p2);
    String lenText = normalized.substring(p2 + 1);
    if (!parseHex32(addrText, ctx.addr)) {
      return false;
    }
    if (!parseDec32(lenText, ctx.len)) {
      return false;
    }
    ctx.cmd = CommandType::Read;
    return true;
  }

  if (op == "PROGRAM_BYTE" || op == "W") {
    int p2 = normalized.indexOf('|', p1 + 1);
    uint32_t value = 0;
    if (p2 < 0) {
      return false;
    }
    String addrText = normalized.substring(p1 + 1, p2);
    String valueText = normalized.substring(p2 + 1);
    if (!parseHex32(addrText, ctx.addr)) {
      return false;
    }
    if (!parseHex32(valueText, value)) {
      return false;
    }
    ctx.value = static_cast<uint8_t>(value & 0xFF);
    ctx.cmd = CommandType::ProgramByte;
    return true;
  }

  if (op == "PROGRAM_RANGE" || op == "G") {
    int p2 = normalized.indexOf('|', p1 + 1);
    if (p2 < 0) {
      return false;
    }
    String addrText = normalized.substring(p1 + 1, p2);
    if (!parseHex32(addrText, ctx.addr)) {
      return false;
    }
    ctx.cmd = CommandType::ProgramRange;
    return true;
  }

  if (op == "SECTOR_ERASE" || op == "E") {
    String addrText = normalized.substring(p1 + 1);
    if (!parseHex32(addrText, ctx.addr)) {
      return false;
    }
    ctx.cmd = CommandType::SectorErase;
    return true;
  }

  if (op == "WRITE_STATUS" || op == "T") {
    int p2 = normalized.indexOf('|', p1 + 1);
    int p3 = normalized.indexOf('|', p2 + 1);
    uint32_t expected = 0;
    if (p2 < 0 || p3 < 0) {
      return false;
    }
    String addrText = normalized.substring(p1 + 1, p2);
    String expectedText = normalized.substring(p2 + 1, p3);
    String timeoutText = normalized.substring(p3 + 1);
    if (!parseHex32(addrText, ctx.addr)) {
      return false;
    }
    if (!parseHex32(expectedText, expected)) {
      return false;
    }
    if (!parseDec32(timeoutText, ctx.timeoutMs)) {
      return false;
    }
    ctx.value = static_cast<uint8_t>(expected & 0xFF);
    ctx.cmd = CommandType::WriteStatus;
    return true;
  }

  if (op == "ADDR_BUS_TEST") {
    String bank = normalized.substring(p1 + 1);
    bank.trim();
    if (bank == "A0_7" || bank == "LOW8") {
      ctx.bank = 0;
    } else if (bank == "A8_15" || bank == "HIGH8") {
      ctx.bank = 1;
    } else if (bank == "A16_18" || bank == "HIGH3") {
      ctx.bank = 2;
    } else {
      return false;
    }
    ctx.cmd = CommandType::AddrBusTest;
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

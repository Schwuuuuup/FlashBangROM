#include "seq_interpreter.h"

#include <cstdlib>
#include <cstring>

#include "device_config.h"
#include "device_globals.h"
#include "hal_bus.h"
#include "driver_ops.h"

namespace {

// Parse a hex value from string, advancing pos. Stops at comma, semicolon,
// '>', '}', or end of string. Returns parsed value via out.
bool parseHexToken(const char* script, uint16_t& pos, uint32_t& out) {
  uint32_t val = 0;
  bool hasDigit = false;
  while (script[pos] != '\0' && script[pos] != ',' && script[pos] != ';' &&
         script[pos] != '>' && script[pos] != '}') {
    char c = script[pos];
    uint8_t d;
    if (c >= '0' && c <= '9') d = c - '0';
    else if (c >= 'A' && c <= 'F') d = 10 + (c - 'A');
    else if (c >= 'a' && c <= 'f') d = 10 + (c - 'a');
    else return false;
    val = (val << 4) | d;
    hasDigit = true;
    pos++;
  }
  out = val;
  return hasDigit;
}

// Parse a decimal value from string, advancing pos. Stops at comma, semicolon,
// '>', '}', or end of string.
bool parseDecToken(const char* script, uint16_t& pos, uint32_t& out) {
  uint32_t val = 0;
  bool hasDigit = false;
  while (script[pos] != '\0' && script[pos] != ',' && script[pos] != ';' &&
         script[pos] != '>' && script[pos] != '}') {
    char c = script[pos];
    if (c < '0' || c > '9') return false;
    val = val * 10 + (c - '0');
    hasDigit = true;
    pos++;
  }
  out = val;
  return hasDigit;
}

// Resolve a value token which may be a hex literal or a variable ($A, $D, $R0, $R1, $0..$7).
// For addr/data hex values and variable references.
bool resolveValue(const char* script, uint16_t& pos, uint32_t& out,
                  uint32_t addr, uint8_t data, uint8_t r0, uint8_t r1) {
  if (script[pos] == '$') {
    pos++;
    char c = script[pos];
    if (c == 'A') {
      out = addr;
      pos++;
      return true;
    }
    if (c == 'D') {
      out = data;
      pos++;
      return true;
    }
    if (c == 'R') {
      pos++;
      if (script[pos] == '0') { out = r0; pos++; return true; }
      if (script[pos] == '1') { out = r1; pos++; return true; }
      return false;
    }
    // Custom params $0..$7
    if (c >= '0' && c <= '7') {
      uint8_t idx = c - '0';
      if (idx < g_driverSlot.custom_param_count) {
        out = g_driverSlot.custom_params[idx].value;
      } else {
        out = 0;
      }
      pos++;
      return true;
    }
    return false;
  }
  return parseHexToken(script, pos, out);
}

// Execute a single semicolon-delimited segment of micro-script.
// Returns false on error (e.g. poll timeout).
bool executeSegment(const char* script, uint16_t& pos,
                    uint32_t addr, uint8_t data,
                    uint8_t& r0, uint8_t& r1) {
  char op = script[pos];
  if (op == '\0' || op == ';' || op == '}') return true;

  if (op == 'W' || op == 'w') {
    // Write: Waddr,data
    pos++;
    uint32_t wAddr = 0;
    if (!resolveValue(script, pos, wAddr, addr, data, r0, r1)) return false;
    if (script[pos] != ',') return false;
    pos++;  // skip comma
    uint32_t wData = 0;
    if (!resolveValue(script, pos, wData, addr, data, r0, r1)) return false;
    writeCycle(wAddr, static_cast<uint8_t>(wData & 0xFF));
    return true;
  }

  if (op == 'R' || op == 'r') {
    // Read: Raddr>Rn
    pos++;
    uint32_t rAddr = 0;
    if (!resolveValue(script, pos, rAddr, addr, data, r0, r1)) return false;
    uint8_t val = readCycle(rAddr);
    if (script[pos] == '>') {
      pos++;  // skip '>'
      if (script[pos] == 'R') {
        pos++;
        if (script[pos] == '0') { r0 = val; pos++; }
        else if (script[pos] == '1') { r1 = val; pos++; }
        else return false;
      } else {
        return false;
      }
    }
    return true;
  }

  if (op == 'D' || op == 'd') {
    // Delay: Dvalue (decimal microseconds)
    pos++;
    uint32_t us = 0;
    if (!parseDecToken(script, pos, us)) return false;
    delayMicroseconds(us);
    return true;
  }

  if (op == 'T' || op == 't') {
    // Toggle-Poll: Taddr,timeout
    pos++;
    uint32_t pAddr = 0;
    if (!resolveValue(script, pos, pAddr, addr, data, r0, r1)) return false;
    if (script[pos] != ',') return false;
    pos++;
    uint32_t timeout = 0;
    if (!parseDecToken(script, pos, timeout)) return false;
    return waitToggleDone(pAddr, timeout);
  }

  if (op == 'P' || op == 'p') {
    // Poll DQ7: Paddr,expected,timeout
    pos++;
    uint32_t pAddr = 0;
    if (!resolveValue(script, pos, pAddr, addr, data, r0, r1)) return false;
    if (script[pos] != ',') return false;
    pos++;
    uint32_t expected = 0;
    if (!resolveValue(script, pos, expected, addr, data, r0, r1)) return false;
    if (script[pos] != ',') return false;
    pos++;
    uint32_t timeout = 0;
    if (!parseDecToken(script, pos, timeout)) return false;
    return waitDq7DoneProgram(pAddr, static_cast<uint8_t>(expected), timeout);
  }

  return false;  // unknown opcode
}

// waitToggleDone and waitDq7DoneProgram are declared in the driver ops header but
// we need them here. They're already linked from driver_ops.cpp.

}  // namespace

SeqResult executeSequence(const char* script, uint32_t addr, uint8_t data) {
  SeqResult result = {true, 0, 0};
  if (script == nullptr || script[0] == '\0') {
    result.ok = false;
    return result;
  }

  uint16_t pos = 0;
  while (script[pos] != '\0') {
    // Skip loop braces in non-loop mode (executeSequence runs everything linearly)
    if (script[pos] == '{' || script[pos] == '}') {
      pos++;
      continue;
    }
    if (script[pos] == ';') {
      pos++;
      continue;
    }
    if (!executeSegment(script, pos, addr, data, result.r0, result.r1)) {
      result.ok = false;
      return result;
    }
  }
  return result;
}

SeqResult executeNamedSequence(const DriverSlot& slot, const char* name,
                               uint32_t addr, uint8_t data) {
  const char* script = findSequence(slot, name);
  if (script == nullptr) {
    return {false, 0, 0};
  }
  return executeSequence(script, addr, data);
}

bool executeProgramRange(const char* script, uint32_t startAddr,
                         const uint8_t* buf, uint32_t len) {
  if (script == nullptr) return false;

  // Find { and } positions
  const char* braceOpen = strchr(script, '{');
  const char* braceClose = strchr(script, '}');
  if (braceOpen == nullptr || braceClose == nullptr || braceClose <= braceOpen) {
    // No loop construct — fall back to running full script per byte
    for (uint32_t i = 0; i < len; i++) {
      SeqResult r = executeSequence(script, startAddr + i, buf[i]);
      if (!r.ok) return false;
    }
    return true;
  }

  // Setup: everything before '{'
  if (braceOpen > script) {
    // Copy setup portion
    uint16_t setupLen = braceOpen - script;
    char setupBuf[MAX_SEQ_SCRIPT];
    if (setupLen >= MAX_SEQ_SCRIPT) setupLen = MAX_SEQ_SCRIPT - 1;
    memcpy(setupBuf, script, setupLen);
    // Remove trailing semicolon if present
    if (setupLen > 0 && setupBuf[setupLen - 1] == ';') setupLen--;
    setupBuf[setupLen] = '\0';
    if (setupLen > 0) {
      SeqResult r = executeSequence(setupBuf, startAddr, buf[0]);
      if (!r.ok) return false;
    }
  }

  // Loop body: between '{' and '}'
  uint16_t bodyLen = braceClose - braceOpen - 1;
  char bodyBuf[MAX_SEQ_SCRIPT];
  if (bodyLen >= MAX_SEQ_SCRIPT) bodyLen = MAX_SEQ_SCRIPT - 1;
  memcpy(bodyBuf, braceOpen + 1, bodyLen);
  bodyBuf[bodyLen] = '\0';

  for (uint32_t i = 0; i < len; i++) {
    SeqResult r = executeSequence(bodyBuf, startAddr + i, buf[i]);
    if (!r.ok) return false;
  }

  // Teardown: everything after '}'
  const char* teardownStart = braceClose + 1;
  if (*teardownStart == ';') teardownStart++;
  if (*teardownStart != '\0') {
    SeqResult r = executeSequence(teardownStart, startAddr + len - 1, buf[len - 1]);
    if (!r.ok) return false;
  }

  return true;
}

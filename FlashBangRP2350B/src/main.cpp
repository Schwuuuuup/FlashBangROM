// FlashBangRP2350B - self-contained SST39SF040 programmer firmware.
//
// A terminal (minicom / tio / PuTTY / TeraTerm ...) is the only client that
// is strictly required. Commands are entered as text lines; the binary
// download/upload commands use XMODEM-CRC, which every common terminal
// supports out of the box.
//
// download/upload stream directly between the chip and the serial link, so no
// large RAM buffer is needed (the full 512 KByte image does not fit alongside
// the Arduino-Pico USB/network stack).
//
// See README.md for the full command reference and wiring notes.

#include <Arduino.h>
#include <stdlib.h>
#include <string.h>

#include "sst39.h"
#include "xmodem.h"

static const uint32_t kLineMax = 96;
static char g_line[kLineMax];
static uint32_t g_lineLen = 0;

// Shared state for the streaming XMODEM callbacks.
static uint32_t g_streamBase = 0;   // chip address of logical index 0
static uint32_t g_uploadFails = 0;  // program/verify failures during upload

// -------- streaming callbacks ----------------------------------------------

static uint8_t downloadSource(uint32_t index) {
  return sst39::readByte(g_streamBase + index);
}

static void uploadSink(uint32_t index, uint8_t value) {
  uint32_t addr = g_streamBase + index;
  if (value != 0xFF) {  // 0xFF needs no programming on an erased cell
    if (!sst39::programByte(addr, value)) {
      ++g_uploadFails;
      return;
    }
  }
  if (sst39::readByte(addr) != value) {
    ++g_uploadFails;
  }
}

// -------- helpers ----------------------------------------------------------

static void printHelp() {
  Serial.println(F("FlashBangRP2350B - SST39SF040 programmer"));
  Serial.println(F("Numbers accept 0x hex or decimal. Addresses are byte addresses."));
  Serial.println();
  Serial.println(F("  help                        show this help"));
  Serial.println(F("  id                          read manufacturer/device ID"));
  Serial.println(F("  read <start> <len>          hex-dump a range to the terminal"));
  Serial.println(F("  write <start> <len> <val>   program range with a constant byte"));
  Serial.println(F("  erase sector <n>            erase 4KB sector n (0..127)"));
  Serial.println(F("  erase addr <addr>           erase the sector containing addr"));
  Serial.println(F("  erase all                   chip erase (whole 512KB)"));
  Serial.println(F("  download all                XMODEM-send the whole chip"));
  Serial.println(F("  download <start> <len>      XMODEM-send a range"));
  Serial.println(F("  upload <start> <len>        XMODEM-receive and program a range"));
  Serial.println();
  Serial.println(F("Note: program/upload do NOT auto-erase. Erase the target first;"));
  Serial.println(F("      each written byte is read back and mismatches are reported."));
}

// Parse a numeric token (hex with 0x, or decimal). Returns false on garbage.
static bool parseNum(const char* tok, uint32_t* out) {
  if (!tok || !*tok) {
    return false;
  }
  char* end = nullptr;
  unsigned long v = strtoul(tok, &end, 0);
  if (end == tok || *end != '\0') {
    return false;
  }
  *out = (uint32_t)v;
  return true;
}

static bool checkRange(uint32_t start, uint32_t len) {
  if (len == 0) {
    Serial.println(F("ERR: length is zero"));
    return false;
  }
  if (start >= sst39::kChipSize || len > sst39::kChipSize ||
      start + len > sst39::kChipSize) {
    Serial.println(F("ERR: range exceeds 512KB chip"));
    return false;
  }
  return true;
}

static void hexDump(uint32_t start, uint32_t len) {
  char buf[80];
  for (uint32_t off = 0; off < len; off += 16) {
    uint32_t row = (len - off < 16) ? (len - off) : 16;
    int n = snprintf(buf, sizeof(buf), "%06lX  ",
                     (unsigned long)(start + off));
    for (uint32_t i = 0; i < 16; ++i) {
      if (i < row) {
        n += snprintf(buf + n, sizeof(buf) - n, "%02X ",
                      sst39::readByte(start + off + i));
      } else {
        n += snprintf(buf + n, sizeof(buf) - n, "   ");
      }
    }
    buf[n++] = ' ';
    for (uint32_t i = 0; i < row; ++i) {
      uint8_t c = sst39::readByte(start + off + i);
      buf[n++] = (c >= 0x20 && c < 0x7F) ? (char)c : '.';
    }
    buf[n] = '\0';
    Serial.println(buf);
  }
}

// Read-back verification against a constant value.
static void verifyConstant(uint32_t start, uint32_t len, uint8_t val) {
  uint32_t mismatches = 0;
  uint32_t firstBad = 0;
  for (uint32_t i = 0; i < len; ++i) {
    if (sst39::readByte(start + i) != val) {
      if (mismatches == 0) {
        firstBad = start + i;
      }
      ++mismatches;
    }
  }
  if (mismatches == 0) {
    Serial.println(F("verify OK"));
  } else {
    Serial.print(F("verify FAILED: "));
    Serial.print(mismatches);
    Serial.print(F(" mismatch(es), first at 0x"));
    Serial.println(firstBad, HEX);
    Serial.println(F("(did you erase the target sectors first?)"));
  }
}

// -------- command handlers --------------------------------------------------

static void cmdId() {
  uint8_t mfr = 0, dev = 0;
  sst39::readId(&mfr, &dev);
  Serial.print(F("Manufacturer: 0x"));
  Serial.print(mfr, HEX);
  Serial.print(F("  Device: 0x"));
  Serial.print(dev, HEX);
  if (mfr == sst39::kManufacturerSST && dev == sst39::kDeviceSF040) {
    Serial.println(F("  (SST39SF040)"));
  } else {
    Serial.println(F("  (unexpected - check wiring)"));
  }
}

static void cmdRead(char* args) {
  char* t1 = strtok(args, " ");
  char* t2 = strtok(nullptr, " ");
  uint32_t start = 0, len = 0;
  if (!parseNum(t1, &start) || !parseNum(t2, &len) || !checkRange(start, len)) {
    if (t1 && t2) {
      Serial.println(F("ERR: usage: read <start> <len>"));
    }
    return;
  }
  hexDump(start, len);
}

static void cmdWrite(char* args) {
  char* t1 = strtok(args, " ");
  char* t2 = strtok(nullptr, " ");
  char* t3 = strtok(nullptr, " ");
  uint32_t start = 0, len = 0, val = 0;
  if (!parseNum(t1, &start) || !parseNum(t2, &len) || !parseNum(t3, &val) ||
      val > 0xFF || !checkRange(start, len)) {
    Serial.println(F("ERR: usage: write <start> <len> <val>"));
    return;
  }
  Serial.print(F("Programming "));
  Serial.print(len);
  Serial.print(F(" bytes of 0x"));
  Serial.print(val, HEX);
  Serial.println(F(" ..."));
  uint32_t fails = 0;
  for (uint32_t i = 0; i < len; ++i) {
    if ((uint8_t)val != 0xFF && !sst39::programByte(start + i, (uint8_t)val)) {
      ++fails;
    }
  }
  if (fails) {
    Serial.print(F("WARN: "));
    Serial.print(fails);
    Serial.println(F(" byte(s) failed to program"));
  }
  verifyConstant(start, len, (uint8_t)val);
}

static void cmdErase(char* args) {
  char* what = strtok(args, " ");
  char* arg = strtok(nullptr, " ");
  if (!what) {
    Serial.println(F("ERR: usage: erase all | sector <n> | addr <addr>"));
    return;
  }
  if (strcmp(what, "all") == 0) {
    Serial.println(F("Chip erase (up to ~250ms) ..."));
    Serial.println(sst39::eraseChip() ? F("done") : F("ERR: erase timeout"));
    return;
  }
  if (strcmp(what, "sector") == 0) {
    uint32_t n = 0;
    if (!parseNum(arg, &n) || n >= sst39::kSectorCount) {
      Serial.println(F("ERR: sector index 0..127"));
      return;
    }
    bool ok = sst39::eraseSector(n * sst39::kSectorSize);
    Serial.println(ok ? F("done") : F("ERR: erase timeout"));
    return;
  }
  if (strcmp(what, "addr") == 0) {
    uint32_t addr = 0;
    if (!parseNum(arg, &addr) || addr >= sst39::kChipSize) {
      Serial.println(F("ERR: address out of range"));
      return;
    }
    bool ok = sst39::eraseSector(addr);
    Serial.println(ok ? F("done") : F("ERR: erase timeout"));
    return;
  }
  Serial.println(F("ERR: usage: erase all | sector <n> | addr <addr>"));
}

static void cmdDownload(char* args) {
  char* t1 = strtok(args, " ");
  char* t2 = strtok(nullptr, " ");
  uint32_t start = 0, len = 0;
  if (t1 && strcmp(t1, "all") == 0) {
    start = 0;
    len = sst39::kChipSize;
  } else if (!parseNum(t1, &start) || !parseNum(t2, &len) ||
             !checkRange(start, len)) {
    Serial.println(F("ERR: usage: download all | download <start> <len>"));
    return;
  }
  g_streamBase = start;
  Serial.println(F("Start XMODEM *receive* in your terminal now."));
  delay(200);
  bool ok = xmodem::send(len, downloadSource);
  Serial.println();
  Serial.println(ok ? F("download complete")
                    : F("ERR: XMODEM send failed/aborted"));
}

static void cmdUpload(char* args) {
  char* t1 = strtok(args, " ");
  char* t2 = strtok(nullptr, " ");
  uint32_t start = 0, len = 0;
  if (!parseNum(t1, &start) || !parseNum(t2, &len) || !checkRange(start, len)) {
    Serial.println(F("ERR: usage: upload <start> <len>"));
    return;
  }
  g_streamBase = start;
  g_uploadFails = 0;
  Serial.println(F("Start XMODEM *send* of your file in your terminal now."));
  delay(200);
  bool ok = xmodem::receive(len, uploadSink);
  Serial.println();
  if (!ok) {
    Serial.println(F("ERR: XMODEM receive failed/aborted"));
    return;
  }
  if (g_uploadFails) {
    Serial.print(F("upload done with "));
    Serial.print(g_uploadFails);
    Serial.println(F(" verify error(s) - erase the target first?"));
  } else {
    Serial.println(F("upload complete, verify OK"));
  }
}

static void dispatch(char* line) {
  char* cmd = strtok(line, " ");
  if (!cmd) {
    return;
  }
  char* rest = strtok(nullptr, "");  // remainder of the line
  if (strcmp(cmd, "help") == 0 || strcmp(cmd, "?") == 0) {
    printHelp();
  } else if (strcmp(cmd, "id") == 0) {
    cmdId();
  } else if (strcmp(cmd, "read") == 0) {
    cmdRead(rest ? rest : (char*)"");
  } else if (strcmp(cmd, "write") == 0) {
    cmdWrite(rest ? rest : (char*)"");
  } else if (strcmp(cmd, "erase") == 0) {
    cmdErase(rest ? rest : (char*)"");
  } else if (strcmp(cmd, "download") == 0) {
    cmdDownload(rest ? rest : (char*)"");
  } else if (strcmp(cmd, "upload") == 0) {
    cmdUpload(rest ? rest : (char*)"");
  } else {
    Serial.print(F("ERR: unknown command '"));
    Serial.print(cmd);
    Serial.println(F("' (type help)"));
  }
  Serial.print(F("> "));
}

// -------- Arduino entry points ---------------------------------------------

void setup() {
  Serial.begin(115200);
  uint32_t start = millis();
  while (!Serial && (millis() - start) < 3000) {
    // wait briefly for the USB host, but do not block forever
  }
  sst39::begin();
  Serial.println();
  Serial.println(F("FlashBangRP2350B ready. Type 'help'."));
  Serial.print(F("> "));
}

void loop() {
  static bool lastWasCR = false;
  int c = Serial.read();
  if (c < 0) {
    return;
  }
  if (c == '\r' || c == '\n') {
    // Accept CR, LF or CRLF as end-of-line. Swallow the LF that follows a CR
    // so a CRLF pair is not treated as two separate (empty) lines.
    if (c == '\n' && lastWasCR) {
      lastWasCR = false;
      return;
    }
    lastWasCR = (c == '\r');
    g_line[g_lineLen] = '\0';
    Serial.println();
    dispatch(g_line);
    g_lineLen = 0;
    return;
  }
  lastWasCR = false;
  if (c == 0x08 || c == 0x7F) {  // backspace / delete
    if (g_lineLen > 0) {
      --g_lineLen;
      Serial.print(F("\b \b"));
    }
    return;
  }
  if (g_lineLen < kLineMax - 1) {
    g_line[g_lineLen++] = (char)c;
    Serial.write((uint8_t)c);  // echo
  }
}

#include "protocol_io.h"

#include <cstdio>

namespace {
char nibbleHex(uint8_t v) {
  return static_cast<char>(v < 10 ? ('0' + v) : ('A' + (v - 10)));
}

String toHex2(uint8_t v) {
  String s;
  s.reserve(2);
  s += nibbleHex((v >> 4) & 0x0F);
  s += nibbleHex(v & 0x0F);
  return s;
}
}  // namespace

void sendOk(const char* cmd, const String& detail) {
  Serial.print("OK|");
  Serial.print(cmd);
  Serial.print("|");
  Serial.println(detail);
}

void sendErr(const char* code, const char* message) {
  Serial.print("ERR|");
  Serial.print(code);
  Serial.print("|");
  Serial.println(message);
}

void sendDataFrameHex(uint32_t addr, const uint8_t* data, uint32_t len) {
  String payload;
  payload.reserve(len * 2);
  for (uint32_t i = 0; i < len; ++i) {
    payload += toHex2(data[i]);
  }

  char header[48];
  snprintf(header, sizeof(header), "DATA|%05lX|%lu|",
           static_cast<unsigned long>(addr), static_cast<unsigned long>(len));
  Serial.print(header);
  Serial.println(payload);
}

void sendStatus(const char* operation, const char* phase, uint32_t progress,
                const String& detail) {
  Serial.print("STATUS|");
  Serial.print(operation);
  Serial.print("|");
  Serial.print(phase);
  Serial.print("|");
  Serial.print(progress);
  Serial.print("|");
  Serial.println(detail);
}

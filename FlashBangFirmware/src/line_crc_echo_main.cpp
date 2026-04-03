#include <Arduino.h>

namespace {
static constexpr size_t LINE_MAX = 160;

char g_lineBuf[LINE_MAX];
size_t g_lineLen = 0;
bool g_lineOverflow = false;

uint8_t crc8Xor(const char* text, size_t len) {
  uint8_t crc = 0;
  for (size_t i = 0; i < len; ++i) {
    crc ^= static_cast<uint8_t>(text[i]);
  }
  return crc;
}

void printLineWithCrc(const char* prefix, const char* text, size_t len) {
  uint8_t crc = crc8Xor(text, len);
  char crcHex[3];
  snprintf(crcHex, sizeof(crcHex), "%02X", crc);

  Serial.print(prefix);
  Serial.print("|");
  Serial.write(text, len);
  Serial.print("|");
  Serial.println(crcHex);
}

bool parseHexByte(const char* text, uint8_t& out) {
  if (strlen(text) != 2) {
    return false;
  }
  char* endPtr = nullptr;
  unsigned long value = strtoul(text, &endPtr, 16);
  if (endPtr == nullptr || *endPtr != '\0' || value > 0xFFUL) {
    return false;
  }
  out = static_cast<uint8_t>(value);
  return true;
}

size_t rtrimAsciiWhitespace(const char* line, size_t len) {
  while (len > 0) {
    char c = line[len - 1];
    if (c == ' ' || c == '\t') {
      len--;
      continue;
    }
    break;
  }
  return len;
}

void printRxDebug(const char* line, size_t len) {
  Serial.print("DBG|RX|LEN=");
  Serial.print(static_cast<unsigned long>(len));
  Serial.print("|TXT=");
  Serial.write(line, len);
  Serial.print("|CRC=");
  char crcHex[3];
  snprintf(crcHex, sizeof(crcHex), "%02X", crc8Xor(line, len));
  Serial.println(crcHex);
}

void processInputLine(const char* line, size_t len) {
  // Input format for 1:1 copy/paste:
  // <PREFIX>|<payload>|<CRC_HEX>
  const size_t normLen = rtrimAsciiWhitespace(line, len);
  printRxDebug(line, len);
  if (normLen != len) {
    Serial.print("DBG|TRIM|RAW=");
    Serial.print(static_cast<unsigned long>(len));
    Serial.print("|NORM=");
    Serial.println(static_cast<unsigned long>(normLen));
  }

  len = normLen;

  const char* firstPipe = static_cast<const char*>(memchr(line, '|', len));
  if (firstPipe == nullptr) {
    Serial.println("ERR|FORMAT|NO_PIPE");
    return;
  }

  const size_t firstIdx = static_cast<size_t>(firstPipe - line);
  if (firstIdx == 0) {
    Serial.println("ERR|FORMAT|EMPTY_PREFIX");
    return;
  }

  char rxPrefix[16];
  const size_t prefixLen = (firstIdx < (sizeof(rxPrefix) - 1)) ? firstIdx : (sizeof(rxPrefix) - 1);
  memcpy(rxPrefix, line, prefixLen);
  rxPrefix[prefixLen] = '\0';

  const char* rest = firstPipe + 1;
  const size_t restLen = len - (firstIdx + 1);
  const char* secondPipe = static_cast<const char*>(memchr(rest, '|', restLen));
  if (secondPipe == nullptr) {
    Serial.println("ERR|FORMAT|NO_SECOND_PIPE");
    return;
  }

  const size_t payloadLen = static_cast<size_t>(secondPipe - rest);
  const char* crcText = secondPipe + 1;

  uint8_t rxCrc = 0;
  if (!parseHexByte(crcText, rxCrc)) {
    Serial.print("ERR|CRC_FMT|GOT=");
    Serial.println(crcText);
    return;
  }

  uint8_t calc = crc8Xor(rest, payloadLen);
  if (calc != rxCrc) {
    char crcHex[3];
    snprintf(crcHex, sizeof(crcHex), "%02X", calc);
    char rxHex[3];
    snprintf(rxHex, sizeof(rxHex), "%02X", rxCrc);
    Serial.print("ERR|CRC_MISMATCH|RX=");
    Serial.print(rxHex);
    Serial.print("|CALC=");
    Serial.println(crcHex);
    return;
  }

  printLineWithCrc("ECHO", rest, payloadLen);
  Serial.print("OK|CRC|PREFIX=");
  Serial.println(rxPrefix);
  Serial.println("OK|CRC");
}

void flushInputLine() {
  const bool hadOverflow = g_lineOverflow;
  if (g_lineLen == 0 || hadOverflow) {
    g_lineLen = 0;
    g_lineOverflow = false;
    if (hadOverflow) {
      Serial.println("ERR|LINE_TOO_LONG");
    }
    return;
  }

  g_lineBuf[g_lineLen] = '\0';
  processInputLine(g_lineBuf, g_lineLen);
  g_lineLen = 0;
  g_lineOverflow = false;
}

}  // namespace

void setup() {
  Serial.begin(115200);
  while (!Serial) {
  }
  delay(100);

  Serial.println("HELLO|line-crc-echo|1.0");

  const char* test = "COPY_PASTE_LINK_TEST";
  printLineWithCrc("OUT", test, strlen(test));
  char crcHex[3];
  snprintf(crcHex, sizeof(crcHex), "%02X", crc8Xor(test, strlen(test)));
  Serial.print("INFO|PASTE_EXACTLY|");
  Serial.print("OUT|");
  Serial.print(test);
  Serial.print("|");
  Serial.println(crcHex);
}

void loop() {
  while (Serial.available()) {
    char c = static_cast<char>(Serial.read());
    if (c == '\r' || c == '\n') {
      flushInputLine();
      continue;
    }
    if (c < 0x20 || c == 0x7F) {
      continue;
    }
    if (g_lineLen < (LINE_MAX - 1)) {
      g_lineBuf[g_lineLen++] = c;
    } else {
      g_lineOverflow = true;
    }
  }
}

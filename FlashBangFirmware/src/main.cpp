#include <Arduino.h>

#include <cstring>

#include "command_executor.h"
#include "command_parser.h"
#include "device_config.h"
#include "device_globals.h"
#include "device_types.h"
#include "hal_bus.h"
#include "protocol_io.h"
#include "version_info.h"

namespace {
static constexpr uint32_t DATA_BUS_MONITOR_INTERVAL_MS = 100;
static constexpr uint16_t RX_LINE_MAX = 224;
static constexpr uint8_t RX_QUEUE_DEPTH = 24;
static constexpr uint8_t MAX_CMD_LINES_PER_LOOP = 8;

const char* kCommandPrefixes[] = {
  "PARAMETER|", "SEQUENCE|", "HELLO", "ID", "INSPECT", "DRIVER_RESET",
  "READ|",      "PROGRAM_BYTE|", "SECTOR_ERASE|", "CHIP_ERASE",
  "WRITE_STATUS|", "ADDR_BUS_TEST|", "DATA_BUS_MONITOR_START",
  "DATA_BUS_MONITOR_STOP", "SET_A", "?"};

char g_rxCurrent[RX_LINE_MAX];
uint16_t g_rxCurrentLen = 0;
bool g_rxCurrentOverflow = false;

char g_rxQueue[RX_QUEUE_DEPTH][RX_LINE_MAX];
uint8_t g_rxQueueHead = 0;
uint8_t g_rxQueueTail = 0;
uint8_t g_rxQueueCount = 0;

void enqueueCurrentLine() {
  if (g_rxCurrentLen == 0 || g_rxCurrentOverflow) {
    g_rxCurrentLen = 0;
    g_rxCurrentOverflow = false;
    return;
  }
  g_rxCurrent[g_rxCurrentLen] = '\0';
  if (g_rxQueueCount < RX_QUEUE_DEPTH) {
    strncpy(g_rxQueue[g_rxQueueTail], g_rxCurrent, RX_LINE_MAX - 1);
    g_rxQueue[g_rxQueueTail][RX_LINE_MAX - 1] = '\0';
    g_rxQueueTail = static_cast<uint8_t>((g_rxQueueTail + 1) % RX_QUEUE_DEPTH);
    g_rxQueueCount++;
  }
  g_rxCurrentLen = 0;
  g_rxCurrentOverflow = false;
}

bool dequeueLine(String& out) {
  if (g_rxQueueCount == 0) {
    return false;
  }
  out = g_rxQueue[g_rxQueueHead];
  g_rxQueueHead = static_cast<uint8_t>((g_rxQueueHead + 1) % RX_QUEUE_DEPTH);
  g_rxQueueCount--;
  return true;
}

void drainSerialToQueue() {
  while (Serial.available()) {
    char c = static_cast<char>(Serial.read());
    if (c == '\n' || c == '\r') {
      enqueueCurrentLine();
      continue;
    }
    // Protocol is ASCII text; drop control/non-printable bytes from terminal noise.
    if (c < 0x20 || c == 0x7F) {
      continue;
    }
    if (g_rxCurrentLen < (RX_LINE_MAX - 1)) {
      g_rxCurrent[g_rxCurrentLen++] = c;
    } else {
      g_rxCurrentOverflow = true;
    }
  }
}

int findEarliestCommandPrefix(const String& line) {
  int best = -1;
  for (const char* prefix : kCommandPrefixes) {
    int idx = line.indexOf(prefix);
    if (idx >= 0 && (best < 0 || idx < best)) {
      best = idx;
    }
  }
  return best;
}

String sanitizeIncomingLine(const String& line) {
  String clean = line;
  clean.trim();

  // Strip common bracketed-paste artifacts that can remain after ESC filtering.
  clean.replace("[200~", "");
  clean.replace("[201~", "");

  int start = findEarliestCommandPrefix(clean);
  if (start > 0) {
    clean = clean.substring(start);
  }

  // Recover the first command if two commands get concatenated in one line.
  int concatIdx = clean.indexOf("PARAMETER|", 1);
  if (concatIdx > 0) {
    clean = clean.substring(0, concatIdx);
  }
  concatIdx = clean.indexOf("SEQUENCE|", 1);
  if (concatIdx > 0) {
    clean = clean.substring(0, concatIdx);
  }

  clean.trim();
  return clean;
}

void processIncomingLine(const String& line) {
  String trimmed = sanitizeIncomingLine(line);

  if (trimmed.length() == 0) {
    return;
  }
  if (trimmed.startsWith("# INSPECT BEGIN")) {
    g_inspectPasteActive = true;
    return;
  }
  if (trimmed.startsWith("# INSPECT END")) {
    bool wasPaste = g_inspectPasteActive;
    g_inspectPasteActive = false;
    if (wasPaste) {
      sendOk("INSPECT_PASTE", "done");
    }
    return;
  }
  if (trimmed.startsWith("#")) {
    return;
  }
  if (trimmed.startsWith("OK|") || trimmed.startsWith("ERR|") ||
      trimmed.startsWith("DATA|") || trimmed.startsWith("HELLO|") ||
      trimmed.startsWith("STATUS|")) {
    return;
  }
  if (g_inspectPasteActive) {
    // During block paste, only command payload lines are relevant.
    if (!trimmed.startsWith("PARAMETER|") && !trimmed.startsWith("SEQUENCE|")) {
      return;
    }
  }

  resetContext();
  if (!parseLine(trimmed, g_ctx)) {
    if (!g_inspectPasteActive) {
      sendErr("E_PARSE", "malformed command");
    }
    return;
  }

  executeCommand(g_ctx);
}

String dataBusToBinary(uint8_t value) {
  String bits;
  bits.reserve(8);
  for (int bit = 7; bit >= 0; --bit) {
    bits += ((value >> bit) & 0x01) ? '1' : '0';
  }
  return bits;
}

}  // namespace

void setup() {
  // Preload inactive level for active-low controls before enabling outputs.
  digitalWrite(PIN_WE, HIGH);
  digitalWrite(PIN_OE, HIGH);
  digitalWrite(PIN_CE, HIGH);

  pinMode(PIN_WE, OUTPUT);
  pinMode(PIN_OE, OUTPUT);
  pinMode(PIN_CE, OUTPUT);

  digitalWrite(PIN_WE, HIGH);
  digitalWrite(PIN_OE, HIGH);
  digitalWrite(PIN_CE, HIGH);

  initAddressBusPins();
  busSetAddress(0x00000);

  setDataBusMode(false);

  Serial.begin(115200);
  while (!Serial) {
  }
  delayMicroseconds(WAIT_POWER_UP_US);

  initDriverSlotDefaults(g_driverSlot);
  g_chipSizeBytes = g_driverSlot.chip_size_bytes;

  g_state = DeviceState::Idle;
  Serial.print("HELLO|");
  Serial.print(firmwareVersionText());
  Serial.println("|0.4.1|driver-upload");
}

void loop() {
  // Decouple RX ingest from command execution to survive bursty terminal paste.
  drainSerialToQueue();

  // Process multiple queued commands per loop to keep up with fast terminal pastes.
  for (uint8_t i = 0; i < MAX_CMD_LINES_PER_LOOP; ++i) {
    if (!dequeueLine(g_line)) {
      break;
    }
    processIncomingLine(g_line);
    g_line = "";
  }

  if (g_dataBusMonitorActive) {
    const uint32_t now = millis();
    if (g_dataBusMonitorLastSampleMs == 0 ||
        (now - g_dataBusMonitorLastSampleMs) >= DATA_BUS_MONITOR_INTERVAL_MS) {
      if (g_dataBusMonitorAddrSet) {
        busSetAddress(g_dataBusMonitorAddr & 0x7FFFFUL);
      }
      const uint8_t sample = busReadData();
      String detail = dataBusToBinary(sample);
      if (g_dataBusMonitorAddrSet) {
        char addrHex[8];
        snprintf(addrHex, sizeof(addrHex), "%05lX",
                 static_cast<unsigned long>(g_dataBusMonitorAddr & 0x7FFFFUL));
        detail = String("A=") + addrHex + ",D=" + detail;
      }
      sendStatus("DATA_BUS", "SAMPLE", 0, detail);
      g_dataBusMonitorLastSampleMs = now;
    }
  }
}

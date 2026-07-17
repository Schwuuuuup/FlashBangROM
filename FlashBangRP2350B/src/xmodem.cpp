#include "xmodem.h"

#include <Arduino.h>

namespace xmodem {
namespace {

static const uint8_t SOH = 0x01;
static const uint8_t EOT = 0x04;
static const uint8_t ACK = 0x06;
static const uint8_t NAK = 0x15;
static const uint8_t CAN = 0x18;
static const uint8_t SUB = 0x1A;  // padding byte for the final block
static const uint8_t CRC = 'C';   // 0x43, receiver requests CRC mode

static const uint32_t kBlockSize = 128;
static const uint8_t kMaxRetries = 10;

uint16_t crc16Ccitt(const uint8_t* data, uint32_t len) {
  uint16_t crc = 0;
  for (uint32_t i = 0; i < len; ++i) {
    crc ^= static_cast<uint16_t>(data[i]) << 8;
    for (uint8_t b = 0; b < 8; ++b) {
      crc = (crc & 0x8000) ? static_cast<uint16_t>((crc << 1) ^ 0x1021)
                           : static_cast<uint16_t>(crc << 1);
    }
  }
  return crc;
}

void flushInput() {
  while (Serial.available()) {
    Serial.read();
  }
}

// Read one byte with a timeout. Returns -1 on timeout.
int readByteTimeout(uint32_t timeoutMs) {
  uint32_t start = millis();
  while ((millis() - start) < timeoutMs) {
    int c = Serial.read();
    if (c >= 0) {
      return c;
    }
  }
  return -1;
}

void sendBlock(uint32_t blockNo, const uint8_t* payload, bool crcMode) {
  Serial.write(SOH);
  Serial.write(static_cast<uint8_t>(blockNo & 0xFF));
  Serial.write(static_cast<uint8_t>(~(blockNo & 0xFF)));
  Serial.write(payload, kBlockSize);
  if (crcMode) {
    uint16_t crc = crc16Ccitt(payload, kBlockSize);
    Serial.write(static_cast<uint8_t>(crc >> 8));
    Serial.write(static_cast<uint8_t>(crc & 0xFF));
  } else {
    uint8_t sum = 0;
    for (uint32_t i = 0; i < kBlockSize; ++i) {
      sum = static_cast<uint8_t>(sum + payload[i]);
    }
    Serial.write(sum);
  }
  Serial.flush();
}

}  // namespace

bool send(uint32_t len, ByteSource src) {
  flushInput();

  // Wait for the receiver to start the transfer. 'C' -> CRC, NAK -> checksum.
  bool crcMode = true;
  int handshake = -1;
  for (uint8_t tries = 0; tries < 60; ++tries) {  // up to ~60 s
    handshake = readByteTimeout(1000);
    if (handshake == CRC) {
      crcMode = true;
      break;
    }
    if (handshake == NAK) {
      crcMode = false;
      break;
    }
    if (handshake == CAN) {
      return false;
    }
  }
  if (handshake != CRC && handshake != NAK) {
    return false;  // receiver never showed up
  }

  uint8_t block[kBlockSize];
  uint32_t pos = 0;
  uint32_t blockNo = 1;
  while (pos < len) {
    uint32_t chunk = len - pos;
    if (chunk > kBlockSize) {
      chunk = kBlockSize;
    }
    for (uint32_t i = 0; i < chunk; ++i) {
      block[i] = src(pos + i);
    }
    if (chunk < kBlockSize) {
      memset(block + chunk, SUB, kBlockSize - chunk);
    }

    bool acked = false;
    for (uint8_t retry = 0; retry < kMaxRetries; ++retry) {
      sendBlock(blockNo, block, crcMode);
      int resp = readByteTimeout(2000);
      if (resp == ACK) {
        acked = true;
        break;
      }
      if (resp == CAN) {
        return false;
      }
      // NAK or timeout -> resend
    }
    if (!acked) {
      return false;
    }
    pos += chunk;
    ++blockNo;
  }

  // End of transmission.
  for (uint8_t retry = 0; retry < kMaxRetries; ++retry) {
    Serial.write(EOT);
    Serial.flush();
    int resp = readByteTimeout(2000);
    if (resp == ACK) {
      return true;
    }
  }
  return false;
}

bool receive(uint32_t len, ByteSink sink) {
  flushInput();

  uint32_t expected = 1;
  bool started = false;

  // Poll the sender with 'C' until the first block (or EOT) arrives.
  for (uint8_t tries = 0; tries < 30 && !started; ++tries) {
    Serial.write(CRC);
    Serial.flush();
    uint32_t start = millis();
    while ((millis() - start) < 3000) {
      int c = Serial.read();
      if (c < 0) {
        continue;
      }
      if (c == SOH) {
        started = true;
        goto have_soh;  // fall into the block loop with SOH already consumed
      }
      if (c == EOT) {
        Serial.write(ACK);
        Serial.flush();
        return true;  // empty transfer
      }
      if (c == CAN) {
        return false;
      }
    }
  }
  if (!started) {
    return false;
  }

have_soh:
  bool haveSoh = true;
  for (;;) {
    int c;
    if (haveSoh) {
      c = SOH;
      haveSoh = false;
    } else {
      c = readByteTimeout(10000);
    }

    if (c < 0) {
      return false;
    }
    if (c == EOT) {
      Serial.write(ACK);
      Serial.flush();
      return true;
    }
    if (c == CAN) {
      return false;
    }
    if (c != SOH) {
      continue;  // ignore stray bytes
    }

    int blk = readByteTimeout(1000);
    int blkInv = readByteTimeout(1000);
    uint8_t payload[kBlockSize];
    uint32_t got = 0;
    while (got < kBlockSize) {
      int d = readByteTimeout(1000);
      if (d < 0) {
        break;
      }
      payload[got++] = static_cast<uint8_t>(d);
    }
    int crcHi = readByteTimeout(1000);
    int crcLo = readByteTimeout(1000);

    bool valid = (blk >= 0) && (blkInv >= 0) && (got == kBlockSize) &&
                 (crcHi >= 0) && (crcLo >= 0) &&
                 ((blk ^ blkInv) == 0xFF);
    if (valid) {
      uint16_t rxCrc = static_cast<uint16_t>((crcHi << 8) | crcLo);
      valid = (rxCrc == crc16Ccitt(payload, kBlockSize));
    }

    if (!valid) {
      flushInput();
      Serial.write(NAK);
      Serial.flush();
      continue;
    }

    uint8_t blockNo = static_cast<uint8_t>(blk);
    if (blockNo == (expected & 0xFF)) {
      uint32_t offset = (expected - 1) * kBlockSize;
      for (uint32_t i = 0; i < kBlockSize; ++i) {
        uint32_t dst = offset + i;
        if (dst < len) {
          sink(dst, payload[i]);
        }
      }
      ++expected;
      Serial.write(ACK);
      Serial.flush();
    } else if (blockNo == ((expected - 1) & 0xFF)) {
      Serial.write(ACK);  // duplicate of previous block -> re-ACK
      Serial.flush();
    } else {
      flushInput();
      Serial.write(NAK);
      Serial.flush();
    }
  }
}

}  // namespace xmodem

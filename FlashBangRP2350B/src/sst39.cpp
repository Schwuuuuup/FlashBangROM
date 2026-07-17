#include "sst39.h"

#include <Arduino.h>
#include <hardware/gpio.h>

#include "pins.h"

namespace sst39 {
namespace {

// Combined 64-bit pin masks (bank0 pads 0..47), computed in begin().
uint64_t g_addrMask = 0;
uint64_t g_dataMask = 0;

// Rough bus delay. One volatile-nop iteration is a handful of CPU cycles;
// at 150 MHz ~6 iterations comfortably exceed the SST39 minimums
// (TWP=40 ns, TWPH=30 ns, TAA<=55 ns). Kept configurable for slow chips.
#ifndef BUS_DELAY_LOOPS
#define BUS_DELAY_LOOPS 6
#endif
inline void busDelay() {
  for (volatile uint32_t i = 0; i < BUS_DELAY_LOOPS; ++i) {
    __asm__ volatile("nop");
  }
}

// End-of-write timeouts (safety net; see protocol doc section 8.3).
static const uint32_t kTimeoutProgramUs = 50UL;         // 5x TBP_max
static const uint32_t kTimeoutSectorEraseUs = 50000UL;  // 2x TSE_max
static const uint32_t kTimeoutChipEraseUs = 250000UL;   // 2.5x TSCE_max

inline void setDataInput() { gpio_set_dir_in_masked64(g_dataMask); }
inline void setDataOutput() { gpio_set_dir_out_masked64(g_dataMask); }

void setAddress(uint32_t addr) {
  uint64_t value = 0;
  for (uint8_t bit = 0; bit < 19; ++bit) {
    if (addr & (1UL << bit)) {
      value |= (1ULL << kAddrGpio[bit]);
    }
  }
  gpio_put_masked64(g_addrMask, value);
}

void writeDataBus(uint8_t data) {
  uint64_t value = 0;
  for (uint8_t bit = 0; bit < 8; ++bit) {
    if (data & (1U << bit)) {
      value |= (1ULL << kDataGpio[bit]);
    }
  }
  gpio_put_masked64(g_dataMask, value);
}

uint8_t readDataBus() {
  uint64_t all = gpio_get_all64();
  uint8_t data = 0;
  for (uint8_t bit = 0; bit < 8; ++bit) {
    if (all & (1ULL << kDataGpio[bit])) {
      data |= (1U << bit);
    }
  }
  return data;
}

// One WE#-controlled bus read. CE# stays LOW for the whole session.
uint8_t busRead(uint32_t addr) {
  gpio_put(kPinWE, 1);  // no write during read
  setDataInput();
  setAddress(addr);
  gpio_put(kPinOE, 0);  // enable output
  busDelay();           // wait TAA / TOE
  uint8_t data = readDataBus();
  gpio_put(kPinOE, 1);  // release bus
  return data;
}

// One WE#-controlled bus write cycle.
void busWrite(uint32_t addr, uint8_t data) {
  gpio_put(kPinOE, 1);  // disable chip output before driving the bus
  setDataOutput();
  setAddress(addr);
  writeDataBus(data);
  busDelay();           // address/data setup
  gpio_put(kPinWE, 0);  // latch address
  busDelay();           // TWP
  gpio_put(kPinWE, 1);  // latch data -> internal op may start
  busDelay();           // TWPH / hold
  setDataInput();       // release the bus again
}

// Toggle-bit (DQ6) end-of-write polling with a timeout safety net.
bool waitToggle(uint32_t addr, uint32_t timeoutUs) {
  uint32_t start = micros();
  uint8_t prev = busRead(addr) & 0x40;
  while ((micros() - start) < timeoutUs) {
    uint8_t cur = busRead(addr) & 0x40;
    if (cur == prev) {
      // No toggle: confirm with two more consistent reads (race guard).
      uint8_t a = busRead(addr) & 0x40;
      uint8_t b = busRead(addr) & 0x40;
      if (a == b) {
        return true;
      }
    }
    prev = cur;
  }
  return false;
}

// SDP unlock preambles (see protocol doc section 4).
inline void unlock() {
  busWrite(0x5555, 0xAA);
  busWrite(0x2AAA, 0x55);
}

}  // namespace

void begin() {
  // Build the combined pin masks.
  g_addrMask = 0;
  for (uint8_t i = 0; i < 19; ++i) {
    g_addrMask |= (1ULL << kAddrGpio[i]);
  }
  g_dataMask = 0;
  for (uint8_t i = 0; i < 8; ++i) {
    g_dataMask |= (1ULL << kDataGpio[i]);
  }

  // Initialise every used pad as SIO output/input.
  for (uint8_t i = 0; i < 19; ++i) {
    gpio_init(kAddrGpio[i]);
    gpio_set_dir(kAddrGpio[i], GPIO_OUT);
    gpio_put(kAddrGpio[i], 0);
  }
  for (uint8_t i = 0; i < 8; ++i) {
    gpio_init(kDataGpio[i]);
    // Pull-downs so a Hi-Z / disconnected bus reads a deterministic 0x00
    // instead of latching high (RP2350 erratum E9). A driven chip easily
    // overrides the weak internal pull.
    gpio_pull_down(kDataGpio[i]);
  }
  gpio_init(kPinCE);
  gpio_init(kPinOE);
  gpio_init(kPinWE);
  gpio_set_dir(kPinCE, GPIO_OUT);
  gpio_set_dir(kPinOE, GPIO_OUT);
  gpio_set_dir(kPinWE, GPIO_OUT);

  // Idle: control lines inactive, then keep CE# permanently LOW as the
  // protocol doc recommends (WE#-controlled cycles).
  gpio_put(kPinWE, 1);
  gpio_put(kPinOE, 1);
  gpio_put(kPinCE, 1);
  setDataInput();

  delayMicroseconds(200);  // TPU-READ / TPU-WRITE >= 100 us
  gpio_put(kPinCE, 0);     // select the chip for the whole session
}

uint8_t readByte(uint32_t addr) { return busRead(addr); }

void readRange(uint32_t addr, uint8_t* buf, uint32_t len) {
  for (uint32_t i = 0; i < len; ++i) {
    buf[i] = busRead(addr + i);
  }
}

bool programByte(uint32_t addr, uint8_t data) {
  unlock();
  busWrite(0x5555, 0xA0);  // byte program command
  busWrite(addr, data);
  return waitToggle(addr, kTimeoutProgramUs);
}

uint32_t programRange(uint32_t addr, const uint8_t* buf, uint32_t len) {
  uint32_t failures = 0;
  for (uint32_t i = 0; i < len; ++i) {
    if (buf[i] == 0xFF) {
      continue;  // erased cell already reads 0xFF
    }
    if (!programByte(addr + i, buf[i])) {
      ++failures;
    }
  }
  return failures;
}

bool eraseSector(uint32_t addr) {
  uint32_t sectorBase = addr & ~(kSectorSize - 1);
  unlock();
  busWrite(0x5555, 0x80);  // erase setup
  unlock();
  busWrite(sectorBase, 0x30);  // sector erase
  return waitToggle(sectorBase, kTimeoutSectorEraseUs);
}

bool eraseChip() {
  unlock();
  busWrite(0x5555, 0x80);  // erase setup
  unlock();
  busWrite(0x5555, 0x10);  // chip erase
  return waitToggle(0x0000, kTimeoutChipEraseUs);
}

void readId(uint8_t* manufacturer, uint8_t* device) {
  unlock();
  busWrite(0x5555, 0x90);  // software ID entry
  delayMicroseconds(1);    // TIDA
  if (manufacturer) {
    *manufacturer = busRead(0x0000);
  }
  if (device) {
    *device = busRead(0x0001);
  }
  busWrite(0x0000, 0xF0);  // software ID exit
  delayMicroseconds(1);
}

}  // namespace sst39

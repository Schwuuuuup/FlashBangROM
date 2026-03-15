#include "hal_bus.h"

#include "device_config.h"
#include "device_globals.h"

namespace {
bool g_dataBusPinsConfigured = false;
bool g_addressBusPinsConfigured = false;

constexpr uint8_t kDataPins[] = {PB8, PB9, PB10, PB11,
                                 PB12, PB13, PB14, PB15};

constexpr uint8_t kAddrPinsPortA[] = {PA0, PA1, PA2, PA3, PA4, PA5,
                                      PA6, PA7, PA8, PA9, PA10};
constexpr uint8_t kAddrPinsPortB[] = {PB0, PB1, PB2, PB3,
                                      PB4, PB5, PB6, PB7};

inline void controlLinesIdle() {
  digitalWrite(PIN_WE, HIGH);
  digitalWrite(PIN_OE, HIGH);
  digitalWrite(PIN_CE, HIGH);
}

void ensureAddressBusPinsOutput() {
  if (g_addressBusPinsConfigured) {
    return;
  }
  for (uint8_t pin : kAddrPinsPortA) {
    pinMode(pin, OUTPUT);
    digitalWrite(pin, LOW);
  }
  for (uint8_t pin : kAddrPinsPortB) {
    pinMode(pin, OUTPUT);
    digitalWrite(pin, LOW);
  }
  g_addressBusPinsConfigured = true;
}
}

void initAddressBusPins() {
  ensureAddressBusPinsOutput();
}

void setDataBusMode(bool output) {
  if (g_dataBusPinsConfigured && g_dataBusIsOutput == output) {
    return;
  }
  for (uint8_t pin : kDataPins) {
    pinMode(pin, output ? OUTPUT : INPUT);
  }
  g_dataBusIsOutput = output;
  g_dataBusPinsConfigured = true;
}

void chipSelect(bool enabled) {
  digitalWrite(PIN_CE, enabled ? LOW : HIGH);
}

void outputEnable(bool enabled) {
  if (enabled) {
    setDataBusMode(false);
  }
  digitalWrite(PIN_OE, enabled ? LOW : HIGH);
}

void writeEnablePulse() {
  digitalWrite(PIN_WE, LOW);
  digitalWrite(PIN_WE, HIGH);
}

void busSetAddress(uint32_t addr) {
  ensureAddressBusPinsOutput();

  uint8_t a0_a7 = static_cast<uint8_t>(addr & 0xFF);
  uint8_t a8_a15 = static_cast<uint8_t>((addr >> 8) & 0xFF);
  uint8_t a16_a18 = static_cast<uint8_t>((addr >> 16) & 0x07);

  GPIOA->ODR &= 0xFFFFFF00;
  GPIOA->ODR |= a0_a7;

  GPIOB->ODR &= 0xFFFFFF00;
  GPIOB->ODR |= a8_a15;

  GPIOA->ODR &= 0xFFFFF8FF;
  GPIOA->ODR |= (a16_a18 << 8);
}

void busSetData(uint8_t value) {
  setDataBusMode(true);
  GPIOB->ODR &= 0xFFFF00FF;
  GPIOB->ODR |= (static_cast<uint16_t>(value) << 8);
}

uint8_t busReadData() {
  setDataBusMode(false);
  return static_cast<uint8_t>((GPIOB->IDR >> 8) & 0xFF);
}

void writeCycle(uint32_t addr, uint8_t data) {
  // WE-driven write cycle: OE stays inactive, CE active only during pulse.
  controlLinesIdle();
  outputEnable(false);
  busSetAddress(addr);
  busSetData(data);
  chipSelect(true);
  writeEnablePulse();
  chipSelect(false);
}

uint8_t readCycle(uint32_t addr) {
  // Datasheet-style read window with CE/OE active during sampling.
  controlLinesIdle();
  setDataBusMode(false);
  chipSelect(true);
  outputEnable(true);
  busSetAddress(addr);
  delayMicroseconds(1);
  const uint8_t value = static_cast<uint8_t>((GPIOB->IDR >> 8) & 0xFF);
  outputEnable(false);
  chipSelect(false);
  return value;
}

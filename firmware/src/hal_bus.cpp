#include "hal_bus.h"

#include "device_config.h"
#include "device_globals.h"

void setDataBusMode(bool output) {
  if (g_dataBusIsOutput == output) {
    return;
  }
  for (int pin = PB8; pin <= PB15; ++pin) {
    pinMode(pin, output ? OUTPUT : INPUT);
    if (!output) {
      digitalWrite(pin, LOW);
    }
  }
  g_dataBusIsOutput = output;
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
  delayMicroseconds(1);
  digitalWrite(PIN_WE, HIGH);
  delayMicroseconds(1);
}

void busSetAddress(uint32_t addr) {
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
  chipSelect(true);
  outputEnable(false);
  busSetAddress(addr);
  busSetData(data);
  writeEnablePulse();
}

uint8_t readCycle(uint32_t addr) {
  chipSelect(true);
  outputEnable(true);
  busSetAddress(addr);
  delayMicroseconds(1);
  return busReadData();
}

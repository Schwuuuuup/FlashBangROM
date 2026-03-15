#pragma once

#include <Arduino.h>

void setDataBusMode(bool output);
void chipSelect(bool enabled);
void outputEnable(bool enabled);
void writeEnablePulse();
void busSetAddress(uint32_t addr);
void busSetData(uint8_t value);
uint8_t busReadData();
void writeCycle(uint32_t addr, uint8_t data);
uint8_t readCycle(uint32_t addr);

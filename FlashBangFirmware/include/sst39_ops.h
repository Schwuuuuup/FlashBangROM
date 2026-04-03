#pragma once

#include <Arduino.h>

#include "device_types.h"

bool waitToggleDone(uint32_t addr, uint32_t timeoutUs);
bool waitDq7DoneProgram(uint32_t addr, uint8_t expected, uint32_t timeoutUs);

bool sst39ProgramByte(uint32_t addr, uint8_t value);
bool sst39SectorErase(uint32_t addr);
bool sst39ChipErase();
Sst39ChipInfo sst39ReadId();
void executeRead(uint32_t addr, uint32_t len);

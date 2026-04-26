#pragma once

#include <Arduino.h>

#include "device_types.h"

bool waitToggleDone(uint32_t addr, uint32_t timeoutUs);
bool waitDq7DoneProgram(uint32_t addr, uint8_t expected, uint32_t timeoutUs);

bool driverProgramByte(uint32_t addr, uint8_t value);
bool driverProgramByte(uint32_t addr, uint8_t value, uint8_t* observed, bool* verifyMismatch);
bool driverSectorErase(uint32_t addr);
bool driverChipErase();
void executeRead(uint32_t addr, uint32_t len, bool sendAck = true);

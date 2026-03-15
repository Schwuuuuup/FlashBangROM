#pragma once

#include <Arduino.h>

void sendOk(const char* cmd, const String& detail);
void sendErr(const char* code, const char* message);
void sendDataFrameHex(uint32_t addr, const uint8_t* data, uint32_t len);

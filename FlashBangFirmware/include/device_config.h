#pragma once

#include <Arduino.h>

// Timing defaults derived from Resources/SST39SF_Protocol_fuer_KI.txt.
static constexpr uint32_t WAIT_POWER_UP_US = 100;
static constexpr uint32_t WAIT_POLL_INTERVAL_US = 1;
static constexpr uint32_t WAIT_POST_PROGRAM_STABLE_US = 1;
static constexpr uint32_t WAIT_ID_MODE_US = 1;

static constexpr uint32_t TIMEOUT_BYTE_PROGRAM_US = 50;
static constexpr uint32_t TIMEOUT_SECTOR_ERASE_US = 50000;
static constexpr uint32_t TIMEOUT_CHIP_ERASE_US = 250000;

// BluePill wiring based on legacy prototype.
static constexpr uint8_t PIN_WE = PA14;
static constexpr uint8_t PIN_OE = PA13;
static constexpr uint8_t PIN_CE = PA15;

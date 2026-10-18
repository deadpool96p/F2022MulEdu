#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif

#include <windows.h>
#include <winsock2.h>
#include <ws2tcpip.h>
#include <string>
#include <string_view>
#include <cstdint>

namespace f22 {

// Configuration loaded from f22_emu.ini
struct Config {
    std::string emulator_host = "127.0.0.1";
    uint16_t redirector_port = 42127;
    uint16_t blaze_port = 42128;
    uint16_t telemetry_port = 443;
    bool reroute_all_ea = true;
    bool force_plain_tcp = false;
    bool log_sockets = true;
    bool bypass_cert_validation = true;
    bool enable_overlay = true;
};

// Global config access
Config& GetConfig();
void LoadConfig();

// Logging
void Log(std::string_view level, std::string_view message);
void LogInfo(std::string_view message);
void LogWarn(std::string_view message);
void LogError(std::string_view message);

// Subsystem Hooks
bool InstallWinsockHooks();
bool InstallCertBypassHooks();
bool InstallDXGIHooks();
void RemoveDXGIHooks();

// Real DXGI Loader
bool InitDXGIProxy();

} // namespace f22

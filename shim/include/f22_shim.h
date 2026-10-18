#pragma once

#ifndef WIN32_LEAN_AND_MEAN
#define WIN32_LEAN_AND_MEAN
#endif

#include <winsock2.h>
#include <ws2tcpip.h>
#include <windows.h>
#include <xinput.h>
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
    uint16_t relay_port = 3659;
    bool reroute_all_ea = true;
    bool force_plain_tcp = false;
    bool force_relay = false;
    bool log_sockets = true;
    bool bypass_cert_validation = true;
    bool enable_overlay = true;
    bool enable_steam = true;
    uint32_t steam_appid = 480;
};

// Real-time UDP Netcode Metrics & Transport Mode
enum class TransportMode {
    DirectP2P,
    ProtoTunnelRelay,
    SteamP2P
};

struct NetcodeMetrics {
    uint32_t ping_ms = 18;
    uint32_t jitter_ms = 2;
    uint32_t packets_sent = 0;
    uint32_t packets_recv = 0;
    uint32_t packets_per_sec = 60;
    TransportMode mode = TransportMode::DirectP2P;
    bool sync_ok = true;
};

NetcodeMetrics GetCurrentNetcodeMetrics();

// Global config access
Config& GetConfig();
void LoadConfig();

// Logging
void SetLogDirectory(const std::string& dir);
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

// Netcode Gamepad Streaming
void TransmitNetcodeGamepadPacket(const XINPUT_GAMEPAD& pad);

} // namespace f22

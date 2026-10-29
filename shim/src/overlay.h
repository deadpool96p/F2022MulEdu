#pragma once
#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <string>
#include <cstdint>

struct SeamlessHudState {
    std::string room_name = "fifa_coop_room";
    std::string status_text = "Searching for Relay...";
    std::string role_text = "Host (Peer 0)";
    uint8_t peer_id = 0;
    bool is_connected = false;
    bool is_peer_connected = false;
    uint32_t rtt_ms = 0;
    uint64_t packets_sent = 0;
    uint64_t packets_recv = 0;
};

void UpdateHudState(const SeamlessHudState& state);
SeamlessHudState GetHudState();

bool InstallDxgiOverlayHooks();
void RemoveDxgiOverlayHooks();

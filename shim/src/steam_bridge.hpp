#pragma once

#include <string>
#include <vector>
#include <cstdint>
#include "f22_shim.h"

namespace f22 {

struct SteamFriendInfo {
    uint64_t steam_id = 0;
    std::string persona_name;
};

class SteamBridge {
public:
    static SteamBridge& Instance();

    bool Initialize(uint32_t app_id = 480);
    void Shutdown();

    bool IsActive() const { return m_active; }
    uint64_t GetLocalSteamID() const { return m_local_steam_id; }
    std::string GetLocalPersonaName() const { return m_persona_name; }
    const std::vector<SteamFriendInfo>& GetFriends() const { return m_friends; }

    void RunCallbacks();
    void SetRichPresenceStatus(const std::string& status, const std::string& room_code = "");
    void OpenOverlayInviteDialog();
    void OpenOverlayFriends();

    // Mode 3: Steam P2P Transport
    bool HasSteamNetworking() const { return m_pSteamNetworking != nullptr; }
    bool SendP2PPacket(uint64_t remote_steam_id, const void* data, uint32_t size, int send_type = 1, int channel = 0);
    bool IsP2PPacketAvailable(uint32_t* msg_size, int channel = 0);
    bool ReadP2PPacket(void* dest, uint32_t dest_size, uint32_t* msg_size, uint64_t* remote_steam_id, int channel = 0);

private:
    SteamBridge() = default;
    ~SteamBridge();

    bool LoadSteamAPI();
    void QueryIdentityAndFriends();
    void PushIdentityToControlServer();
    void CheckLaunchCommandLine();

    HMODULE m_hSteamDll = nullptr;
    bool m_active = false;
    uint64_t m_local_steam_id = 0;
    std::string m_persona_name = "SteamUser";
    std::vector<SteamFriendInfo> m_friends;
    std::string m_last_connected_room;

    void* m_pSteamUser = nullptr;
    void* m_pSteamFriends = nullptr;
    void* m_pSteamMatchmaking = nullptr;
    void* m_pSteamNetworking = nullptr;
};

} // namespace f22

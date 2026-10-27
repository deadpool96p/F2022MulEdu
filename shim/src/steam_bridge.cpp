#include "steam_bridge.hpp"
#include "f22_shim.h"
#include "control_client.h"
#include <steam_api_flat.h>
#include <winhttp.h>
#include <fstream>
#include <sstream>
#include <thread>

namespace f22 {

static PFN_SteamAPI_Init s_pfn_SteamAPI_Init = nullptr;
static PFN_SteamAPI_Shutdown s_pfn_SteamAPI_Shutdown = nullptr;
static PFN_SteamAPI_RunCallbacks s_pfn_SteamAPI_RunCallbacks = nullptr;
static PFN_SteamAPI_GetHSteamUser s_pfn_SteamAPI_GetHSteamUser = nullptr;
static PFN_SteamInternal_FindOrCreateUserInterface s_pfn_SteamInternal_FindOrCreateUserInterface = nullptr;

static PFN_SteamAPI_ISteamUser_GetSteamID s_pfn_ISteamUser_GetSteamID = nullptr;
static PFN_SteamAPI_ISteamFriends_GetPersonaName s_pfn_ISteamFriends_GetPersonaName = nullptr;
static PFN_SteamAPI_ISteamFriends_GetFriendCount s_pfn_ISteamFriends_GetFriendCount = nullptr;
static PFN_SteamAPI_ISteamFriends_GetFriendByIndex s_pfn_ISteamFriends_GetFriendByIndex = nullptr;
static PFN_SteamAPI_ISteamFriends_GetFriendPersonaName s_pfn_ISteamFriends_GetFriendPersonaName = nullptr;
static PFN_SteamAPI_ISteamFriends_SetRichPresence s_pfn_ISteamFriends_SetRichPresence = nullptr;
static PFN_SteamAPI_ISteamFriends_ActivateGameOverlay s_pfn_ISteamFriends_ActivateGameOverlay = nullptr;
static PFN_SteamAPI_ISteamFriends_ActivateGameOverlayInviteDialog s_pfn_ISteamFriends_ActivateGameOverlayInviteDialog = nullptr;

static PFN_SteamAPI_ISteamNetworking_SendP2PPacket s_pfn_ISteamNetworking_SendP2PPacket = nullptr;
static PFN_SteamAPI_ISteamNetworking_IsP2PPacketAvailable s_pfn_ISteamNetworking_IsP2PPacketAvailable = nullptr;
static PFN_SteamAPI_ISteamNetworking_ReadP2PPacket s_pfn_ISteamNetworking_ReadP2PPacket = nullptr;
static PFN_SteamAPI_ISteamNetworking_AcceptP2PSessionWithUser s_pfn_ISteamNetworking_AcceptP2PSessionWithUser = nullptr;

SteamBridge& SteamBridge::Instance() {
    static SteamBridge s_instance;
    return s_instance;
}

SteamBridge::~SteamBridge() {
    Shutdown();
}

bool SteamBridge::Initialize(uint32_t app_id) {
    if (m_active) return true;

    // 1. Ensure steam_appid.txt exists in current directory
    std::ifstream check_file("steam_appid.txt");
    if (!check_file.is_open()) {
        std::ofstream out_file("steam_appid.txt");
        if (out_file.is_open()) {
            out_file << app_id;
            out_file.close();
            LogInfo("Created steam_appid.txt with AppID " + std::to_string(app_id));
        }
    } else {
        check_file.close();
    }

    // Set environment variables for Valve loader
    std::string app_str = std::to_string(app_id);
    SetEnvironmentVariableA("SteamAppId", app_str.c_str());
    SetEnvironmentVariableA("SteamGameId", app_str.c_str());

    // 2. Load steam_api64.dll dynamically
    if (!LoadSteamAPI()) {
        LogWarn("steam_api64.dll could not be loaded; falling back to offline emulator persona mode.");
        return false;
    }

    // 3. Initialize SteamAPI
    if (!s_pfn_SteamAPI_Init || !s_pfn_SteamAPI_Init()) {
        LogWarn("SteamAPI_Init() failed. Ensure Steam desktop client is running and AppID " +
                app_str + " (Spacewar) is authorized. Operating in standalone emulator mode.");
        return false;
    }

    m_active = true;
    LogInfo("Steamworks (AppID " + app_str + ") initialized successfully!");

    // 4. Query identity & friends
    QueryIdentityAndFriends();

    // 5. Push profile to control server
    PushIdentityToControlServer();

    // 6. Set initial Rich Presence
    SetRichPresenceStatus("Playing FIFA 22 Online");

    // 7. Check if launched with +connect_room
    CheckLaunchCommandLine();

    return true;
}

void SteamBridge::Shutdown() {
    if (!m_active) return;
    m_active = false;

    if (s_pfn_SteamAPI_Shutdown) {
        s_pfn_SteamAPI_Shutdown();
    }

    if (m_hSteamDll) {
        FreeLibrary(m_hSteamDll);
        m_hSteamDll = nullptr;
    }

    LogInfo("Steamworks bridge shutdown cleanly.");
}

bool SteamBridge::LoadSteamAPI() {
    m_hSteamDll = LoadLibraryA("steam_api64.dll");
    if (!m_hSteamDll) {
        // Try searching in shim/vendor/steam/bin/win64 or parent directory
        m_hSteamDll = LoadLibraryA("shim\\vendor\\steam\\bin\\win64\\steam_api64.dll");
    }
    if (!m_hSteamDll) return false;

    s_pfn_SteamAPI_Init = reinterpret_cast<PFN_SteamAPI_Init>(GetProcAddress(m_hSteamDll, "SteamAPI_Init"));
    s_pfn_SteamAPI_Shutdown = reinterpret_cast<PFN_SteamAPI_Shutdown>(GetProcAddress(m_hSteamDll, "SteamAPI_Shutdown"));
    s_pfn_SteamAPI_RunCallbacks = reinterpret_cast<PFN_SteamAPI_RunCallbacks>(GetProcAddress(m_hSteamDll, "SteamAPI_RunCallbacks"));
    s_pfn_SteamAPI_GetHSteamUser = reinterpret_cast<PFN_SteamAPI_GetHSteamUser>(GetProcAddress(m_hSteamDll, "SteamAPI_GetHSteamUser"));
    s_pfn_SteamInternal_FindOrCreateUserInterface = reinterpret_cast<PFN_SteamInternal_FindOrCreateUserInterface>(
        GetProcAddress(m_hSteamDll, "SteamInternal_FindOrCreateUserInterface"));

    s_pfn_ISteamUser_GetSteamID = reinterpret_cast<PFN_SteamAPI_ISteamUser_GetSteamID>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamUser_GetSteamID"));
    s_pfn_ISteamFriends_GetPersonaName = reinterpret_cast<PFN_SteamAPI_ISteamFriends_GetPersonaName>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamFriends_GetPersonaName"));
    s_pfn_ISteamFriends_GetFriendCount = reinterpret_cast<PFN_SteamAPI_ISteamFriends_GetFriendCount>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamFriends_GetFriendCount"));
    s_pfn_ISteamFriends_GetFriendByIndex = reinterpret_cast<PFN_SteamAPI_ISteamFriends_GetFriendByIndex>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamFriends_GetFriendByIndex"));
    s_pfn_ISteamFriends_GetFriendPersonaName = reinterpret_cast<PFN_SteamAPI_ISteamFriends_GetFriendPersonaName>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamFriends_GetFriendPersonaName"));
    s_pfn_ISteamFriends_SetRichPresence = reinterpret_cast<PFN_SteamAPI_ISteamFriends_SetRichPresence>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamFriends_SetRichPresence"));
    s_pfn_ISteamFriends_ActivateGameOverlay = reinterpret_cast<PFN_SteamAPI_ISteamFriends_ActivateGameOverlay>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamFriends_ActivateGameOverlay"));
    s_pfn_ISteamFriends_ActivateGameOverlayInviteDialog = reinterpret_cast<PFN_SteamAPI_ISteamFriends_ActivateGameOverlayInviteDialog>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamFriends_ActivateGameOverlayInviteDialog"));

    s_pfn_ISteamNetworking_SendP2PPacket = reinterpret_cast<PFN_SteamAPI_ISteamNetworking_SendP2PPacket>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamNetworking_SendP2PPacket"));
    s_pfn_ISteamNetworking_IsP2PPacketAvailable = reinterpret_cast<PFN_SteamAPI_ISteamNetworking_IsP2PPacketAvailable>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamNetworking_IsP2PPacketAvailable"));
    s_pfn_ISteamNetworking_ReadP2PPacket = reinterpret_cast<PFN_SteamAPI_ISteamNetworking_ReadP2PPacket>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamNetworking_ReadP2PPacket"));
    s_pfn_ISteamNetworking_AcceptP2PSessionWithUser = reinterpret_cast<PFN_SteamAPI_ISteamNetworking_AcceptP2PSessionWithUser>(
        GetProcAddress(m_hSteamDll, "SteamAPI_ISteamNetworking_AcceptP2PSessionWithUser"));

    return (s_pfn_SteamAPI_Init != nullptr && s_pfn_SteamAPI_RunCallbacks != nullptr);
}

void SteamBridge::QueryIdentityAndFriends() {
    if (!m_active || !s_pfn_SteamAPI_GetHSteamUser || !s_pfn_SteamInternal_FindOrCreateUserInterface) return;

    HSteamUser hUser = s_pfn_SteamAPI_GetHSteamUser();

    // Query ISteamUser
    const char* user_versions[] = { "SteamUser021", "SteamUser020", "SteamUser019", "SteamUser" };
    for (const char* ver : user_versions) {
        m_pSteamUser = s_pfn_SteamInternal_FindOrCreateUserInterface(hUser, ver);
        if (m_pSteamUser) break;
    }

    if (m_pSteamUser && s_pfn_ISteamUser_GetSteamID) {
        m_local_steam_id = s_pfn_ISteamUser_GetSteamID(m_pSteamUser);
    }

    // Query ISteamFriends
    const char* friends_versions[] = { "SteamFriends017", "SteamFriends016", "SteamFriends" };
    for (const char* ver : friends_versions) {
        m_pSteamFriends = s_pfn_SteamInternal_FindOrCreateUserInterface(hUser, ver);
        if (m_pSteamFriends) break;
    }

    // Query ISteamNetworking (for Mode 3 Steam P2P Transport)
    const char* net_versions[] = { "SteamNetworking006", "SteamNetworking005", "SteamNetworking004", "SteamNetworking" };
    for (const char* ver : net_versions) {
        m_pSteamNetworking = s_pfn_SteamInternal_FindOrCreateUserInterface(hUser, ver);
        if (m_pSteamNetworking) break;
    }

    if (m_pSteamFriends) {
        if (s_pfn_ISteamFriends_GetPersonaName) {
            const char* name = s_pfn_ISteamFriends_GetPersonaName(m_pSteamFriends);
            if (name && strlen(name) > 0) {
                m_persona_name = name;
            }
        }

        m_friends.clear();
        if (s_pfn_ISteamFriends_GetFriendCount && s_pfn_ISteamFriends_GetFriendByIndex && s_pfn_ISteamFriends_GetFriendPersonaName) {
            int count = s_pfn_ISteamFriends_GetFriendCount(m_pSteamFriends, k_EFriendFlagImmediate);
            for (int i = 0; i < count; ++i) {
                uint64_t fid = s_pfn_ISteamFriends_GetFriendByIndex(m_pSteamFriends, i, k_EFriendFlagImmediate);
                const char* fname = s_pfn_ISteamFriends_GetFriendPersonaName(m_pSteamFriends, fid);
                if (fname) {
                    m_friends.push_back({ fid, fname });
                }
            }
        }
    }

    LogInfo("Steam profile: " + m_persona_name + " (SteamID: " + std::to_string(m_local_steam_id) +
            "), Friends online: " + std::to_string(m_friends.size()));
}

void SteamBridge::PushIdentityToControlServer() {
    std::thread([this]() {
        HINTERNET hSession = WinHttpOpen(L"FIFA22-SteamShim/1.0",
                                         WINHTTP_ACCESS_TYPE_NO_PROXY,
                                         WINHTTP_NO_PROXY_NAME,
                                         WINHTTP_NO_PROXY_BYPASS, 0);
        if (!hSession) return;

        std::wstring wHost(GetConfig().emulator_host.begin(), GetConfig().emulator_host.end());
        HINTERNET hConnect = WinHttpConnect(hSession, wHost.c_str(), 42129, 0);
        if (!hConnect) {
            WinHttpCloseHandle(hSession);
            return;
        }

        HINTERNET hRequest = WinHttpOpenRequest(hConnect, L"POST", L"/api/v1/steam/identity",
                                               nullptr, WINHTTP_NO_REFERER,
                                               WINHTTP_DEFAULT_ACCEPT_TYPES, 0);
        if (!hRequest) {
            WinHttpCloseHandle(hConnect);
            WinHttpCloseHandle(hSession);
            return;
        }

        std::string persona = m_persona_name.empty() ? "DEad>_<POol" : m_persona_name;
        std::string sid_str = (m_local_steam_id == 0) ? "76561198956360709" : std::to_string(m_local_steam_id);

        // Build JSON payload matching exactly {"name":"...","id":"..."}
        std::ostringstream json;
        json << "{\"name\":\"" << persona << "\",\"id\":\"" << sid_str << "\",\"friends\":[";
        for (size_t i = 0; i < m_friends.size(); ++i) {
            if (i > 0) json << ",";
            json << "{\"id\":\"" << m_friends[i].steam_id << "\",\"name\":\"" << m_friends[i].persona_name << "\"}";
        }
        json << "]}";

        std::string body = json.str();
        LPCWSTR headers = L"Content-Type: application/json\r\n";

        WinHttpSetTimeouts(hRequest, 2000, 2000, 2000, 2000);
        if (WinHttpSendRequest(hRequest, headers, (DWORD)-1L,
                               (LPVOID)body.data(), (DWORD)body.length(), (DWORD)body.length(), 0) &&
            WinHttpReceiveResponse(hRequest, nullptr)) {
            LogInfo("Pushed Steam identity and friends list to control server (:42129).");
        }

        WinHttpCloseHandle(hRequest);
        WinHttpCloseHandle(hConnect);
        WinHttpCloseHandle(hSession);
    }).detach();
}

void SteamBridge::RunCallbacks() {
    if (!m_active) return;

    if (s_pfn_SteamAPI_RunCallbacks) {
        s_pfn_SteamAPI_RunCallbacks();
    }
}

void SteamBridge::SetRichPresenceStatus(const std::string& status, const std::string& room_code) {
    if (!m_active || !m_pSteamFriends || !s_pfn_ISteamFriends_SetRichPresence) return;

    s_pfn_ISteamFriends_SetRichPresence(m_pSteamFriends, "status", status.c_str());

    if (!room_code.empty()) {
        std::string connect_arg = "+connect_room " + room_code;
        s_pfn_ISteamFriends_SetRichPresence(m_pSteamFriends, "connect", connect_arg.c_str());
    } else {
        s_pfn_ISteamFriends_SetRichPresence(m_pSteamFriends, "connect", "");
    }
}

void SteamBridge::OpenOverlayInviteDialog() {
    if (!m_active || !m_pSteamFriends) return;

    if (s_pfn_ISteamFriends_ActivateGameOverlayInviteDialog) {
        s_pfn_ISteamFriends_ActivateGameOverlayInviteDialog(m_pSteamFriends, 0);
    } else if (s_pfn_ISteamFriends_ActivateGameOverlay) {
        s_pfn_ISteamFriends_ActivateGameOverlay(m_pSteamFriends, "friends");
    }
}

void SteamBridge::OpenOverlayFriends() {
    if (!m_active || !m_pSteamFriends || !s_pfn_ISteamFriends_ActivateGameOverlay) return;
    s_pfn_ISteamFriends_ActivateGameOverlay(m_pSteamFriends, "friends");
}

void SteamBridge::CheckLaunchCommandLine() {
    LPSTR cmdLine = GetCommandLineA();
    if (!cmdLine) return;

    std::string line(cmdLine);
    std::string key = "+connect_room ";
    size_t pos = line.find(key);
    if (pos != std::string::npos) {
        pos += key.length();
        size_t end = line.find_first_of(" \t\r\n\"", pos);
        std::string code = (end == std::string::npos) ? line.substr(pos) : line.substr(pos, end - pos);
        if (!code.empty() && code != m_last_connected_room) {
            m_last_connected_room = code;
            LogInfo("Detected +connect_room on launch: " + code + ". Auto-joining room...");
            ControlClient::Instance().RequestJoinRoom(code);
        }
    }
}

bool SteamBridge::SendP2PPacket(uint64_t remote_steam_id, const void* data, uint32_t size, int send_type, int channel) {
    if (!m_active || !m_pSteamNetworking || !s_pfn_ISteamNetworking_SendP2PPacket) return false;
    return s_pfn_ISteamNetworking_SendP2PPacket(m_pSteamNetworking, remote_steam_id, data, size, send_type, channel);
}

bool SteamBridge::IsP2PPacketAvailable(uint32_t* msg_size, int channel) {
    if (!m_active || !m_pSteamNetworking || !s_pfn_ISteamNetworking_IsP2PPacketAvailable) return false;
    return s_pfn_ISteamNetworking_IsP2PPacketAvailable(m_pSteamNetworking, msg_size, channel);
}

bool SteamBridge::ReadP2PPacket(void* dest, uint32_t dest_size, uint32_t* msg_size, uint64_t* remote_steam_id, int channel) {
    if (!m_active || !m_pSteamNetworking || !s_pfn_ISteamNetworking_ReadP2PPacket) return false;
    return s_pfn_ISteamNetworking_ReadP2PPacket(m_pSteamNetworking, dest, dest_size, msg_size, remote_steam_id, channel);
}

bool SteamBridge::AcceptP2PSession(uint64_t remote_steam_id) {
    if (!m_active || !m_pSteamNetworking || !s_pfn_ISteamNetworking_AcceptP2PSessionWithUser) return false;
    return s_pfn_ISteamNetworking_AcceptP2PSessionWithUser(m_pSteamNetworking, remote_steam_id);
}

} // namespace f22

#pragma once

#include <cstdint>
#include <cstddef>

#ifdef __cplusplus
extern "C" {
#endif

// Core Steam Types
typedef int32_t HSteamUser;
typedef int32_t HSteamPipe;
typedef uint64_t CSteamID_t;
typedef uint64_t SteamAPICall_t;

// EFriendFlags
enum EFriendFlags {
    k_EFriendFlagNone = 0x00,
    k_EFriendFlagBlocked = 0x01,
    k_EFriendFlagFriendshipRequested = 0x02,
    k_EFriendFlagImmediate = 0x04,
    k_EFriendFlagClanMember = 0x08,
    k_EFriendFlagOnGamepad = 0x10,
    k_EFriendFlagRequestingFriendship = 0x20,
    k_EFriendFlagRequestingInfo = 0x40,
    k_EFriendFlagIgnored = 0x80,
    k_EFriendFlagIgnoredFriend = 0x100,
    k_EFriendFlagSuggested = 0x200,
    k_EFriendFlagAll = 0xFFFF,
};

// ELobbyType
enum ELobbyType {
    k_ELobbyTypePrivate = 0,
    k_ELobbyTypeFriendsOnly = 1,
    k_ELobbyTypePublic = 2,
    k_ELobbyTypeInvisible = 3,
};

// Flat C Function Pointer Signatures
typedef bool (*PFN_SteamAPI_Init)(void);
typedef void (*PFN_SteamAPI_Shutdown)(void);
typedef void (*PFN_SteamAPI_RunCallbacks)(void);
typedef HSteamUser (*PFN_SteamAPI_GetHSteamUser)(void);
typedef HSteamPipe (*PFN_SteamAPI_GetHSteamPipe)(void);
typedef void* (*PFN_SteamInternal_FindOrCreateUserInterface)(HSteamUser hSteamUser, const char* pszVersion);
typedef void* (*PFN_SteamInternal_CreateInterface)(const char* pszVersion);

// ISteamUser Flat Functions
typedef uint64_t (*PFN_SteamAPI_ISteamUser_GetSteamID)(void* self);
typedef bool (*PFN_SteamAPI_ISteamUser_BLoggedOn)(void* self);

// ISteamFriends Flat Functions
typedef const char* (*PFN_SteamAPI_ISteamFriends_GetPersonaName)(void* self);
typedef int (*PFN_SteamAPI_ISteamFriends_GetFriendCount)(void* self, int iFriendFlags);
typedef uint64_t (*PFN_SteamAPI_ISteamFriends_GetFriendByIndex)(void* self, int iFriend, int iFriendFlags);
typedef const char* (*PFN_SteamAPI_ISteamFriends_GetFriendPersonaName)(void* self, uint64_t steamIDFriend);
typedef bool (*PFN_SteamAPI_ISteamFriends_SetRichPresence)(void* self, const char* pchKey, const char* pchValue);
typedef void (*PFN_SteamAPI_ISteamFriends_ActivateGameOverlay)(void* self, const char* pchDialog);
typedef void (*PFN_SteamAPI_ISteamFriends_ActivateGameOverlayInviteDialog)(void* self, uint64_t steamIDLobby);

// ISteamMatchmaking Flat Functions
typedef SteamAPICall_t (*PFN_SteamAPI_ISteamMatchmaking_CreateLobby)(void* self, int eLobbyType, int cMaxMembers);
typedef SteamAPICall_t (*PFN_SteamAPI_ISteamMatchmaking_JoinLobby)(void* self, uint64_t steamIDLobby);
typedef bool (*PFN_SteamAPI_ISteamMatchmaking_SetLobbyData)(void* self, uint64_t steamIDLobby, const char* pchKey, const char* pchValue);
typedef const char* (*PFN_SteamAPI_ISteamMatchmaking_GetLobbyData)(void* self, uint64_t steamIDLobby, const char* pchKey);

#ifdef __cplusplus
}
#endif

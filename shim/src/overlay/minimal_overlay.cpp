#include "minimal_overlay.h"
#include "input_hook.h"
#include "../control_client.h"
#include "../steam_bridge.hpp"
#include <imgui.h>
#include <chrono>
#include <string>
#include <algorithm>

namespace f22 {

static char s_room_code_input[16] = "";
static std::string s_feedback_msg = "";
static std::chrono::steady_clock::time_point s_feedback_expire;

static void SetFeedback(const std::string& msg, int duration_ms = 3000) {
    s_feedback_msg = msg;
    s_feedback_expire = std::chrono::steady_clock::now() + std::chrono::milliseconds(duration_ms);
}

void InitOverlayStyle() {
    ImGuiStyle& style = ImGui::GetStyle();
    style.WindowRounding = 6.0f;
    style.FrameRounding = 4.0f;
    style.GrabRounding = 4.0f;
    style.WindowBorderSize = 1.0f;
    style.FrameBorderSize = 1.0f;
    style.ItemSpacing = ImVec2(10.0f, 6.0f);
    style.ItemInnerSpacing = ImVec2(6.0f, 4.0f);
    style.WindowPadding = ImVec2(12.0f, 8.0f);

    ImVec4* colors = style.Colors;
    colors[ImGuiCol_WindowBg]         = ImVec4(0.06f, 0.07f, 0.09f, 0.92f);
    colors[ImGuiCol_Border]           = ImVec4(0.20f, 0.25f, 0.35f, 0.65f);
    colors[ImGuiCol_FrameBg]          = ImVec4(0.12f, 0.14f, 0.18f, 0.90f);
    colors[ImGuiCol_FrameBgHovered]   = ImVec4(0.18f, 0.22f, 0.28f, 1.00f);
    colors[ImGuiCol_FrameBgActive]    = ImVec4(0.22f, 0.28f, 0.36f, 1.00f);
    colors[ImGuiCol_TitleBg]          = ImVec4(0.08f, 0.09f, 0.12f, 1.00f);
    colors[ImGuiCol_TitleBgActive]    = ImVec4(0.10f, 0.12f, 0.16f, 1.00f);
    colors[ImGuiCol_Button]           = ImVec4(0.16f, 0.32f, 0.55f, 0.85f);
    colors[ImGuiCol_ButtonHovered]    = ImVec4(0.22f, 0.44f, 0.72f, 1.00f);
    colors[ImGuiCol_ButtonActive]     = ImVec4(0.28f, 0.52f, 0.82f, 1.00f);
    colors[ImGuiCol_Text]             = ImVec4(0.92f, 0.94f, 0.96f, 1.00f);
    colors[ImGuiCol_TextDisabled]     = ImVec4(0.50f, 0.55f, 0.60f, 1.00f);
}

static void RenderStatusPill(const ServerStatus& status) {
    if (!IsStatusPillVisible()) return;

    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImVec2 work_pos = viewport->WorkPos;
    ImVec2 work_size = viewport->WorkSize;

    NetcodeMetrics netcode = GetCurrentNetcodeMetrics();

    // Dynamically sized pill for full netcode telemetry
    float pill_width = 300.0f;
    float pill_height = 34.0f;
    ImVec2 pill_pos = ImVec2(work_pos.x + work_size.x - pill_width - 16.0f, work_pos.y + 12.0f);

    ImGui::SetNextWindowPos(pill_pos, ImGuiCond_Always);
    ImGui::SetNextWindowSize(ImVec2(pill_width, pill_height), ImGuiCond_Always);

    ImGuiWindowFlags flags = ImGuiWindowFlags_NoDecoration |
                            ImGuiWindowFlags_NoMove |
                            ImGuiWindowFlags_NoResize |
                            ImGuiWindowFlags_NoSavedSettings |
                            ImGuiWindowFlags_NoFocusOnAppearing |
                            ImGuiWindowFlags_NoNav |
                            ImGuiWindowFlags_NoInputs;

    ImGui::PushStyleColor(ImGuiCol_WindowBg, ImVec4(0.05f, 0.07f, 0.10f, 0.85f));
    ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, 17.0f);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(12.0f, 6.0f));

    if (ImGui::Begin("##StatusPill", nullptr, flags)) {
        ImDrawList* draw_list = ImGui::GetWindowDrawList();
        ImVec2 p = ImGui::GetCursorScreenPos();

        // Status Indicator Dot
        ImU32 dot_color = IM_COL32(231, 76, 60, 255); // Red (Offline)
        const char* mode_str = "P2P";

        if (netcode.mode == TransportMode::ProtoTunnelRelay) {
            dot_color = IM_COL32(241, 196, 15, 255); // Amber (Relay)
            mode_str = "RELAY";
        } else if (netcode.mode == TransportMode::SteamP2P) {
            dot_color = IM_COL32(52, 152, 219, 255); // Blue (Steam P2P)
            mode_str = "STEAM-P2P";
        } else {
            dot_color = IM_COL32(46, 204, 113, 255); // Green (Direct P2P)
            mode_str = "P2P";
        }

        // Draw glowing dot
        draw_list->AddCircleFilled(ImVec2(p.x + 4.0f, p.y + 10.0f), 5.0f, dot_color);

        ImGui::SetCursorScreenPos(ImVec2(p.x + 16.0f, p.y + 2.0f));
        if (status.online || netcode.packets_recv > 0) {
            uint32_t display_ping = netcode.ping_ms > 0 ? netcode.ping_ms : (status.peer_ping_ms > 0 ? status.peer_ping_ms : 18);
            uint32_t display_jitter = netcode.jitter_ms;
            ImGui::TextColored(
                ImVec4(0.9f, 0.95f, 1.0f, 1.0f),
                "ONLINE | %ums (±%ums) | %s | Sync: %s",
                display_ping,
                display_jitter,
                mode_str,
                netcode.sync_ok ? "OK" : "DESYNC"
            );
        } else {
            ImGui::TextColored(ImVec4(0.8f, 0.8f, 0.8f, 1.0f), "EMULATOR OFFLINE");
        }
    }
    ImGui::End();

    ImGui::PopStyleVar(2);
    ImGui::PopStyleColor();
}

static void RenderTopBar(const ServerStatus& status) {
    if (!IsOverlayFocused()) return;

    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImVec2 bar_pos = viewport->WorkPos;
    float bar_height = 46.0f;
    ImVec2 bar_size = ImVec2(viewport->WorkSize.x, bar_height);

    ImGui::SetNextWindowPos(bar_pos, ImGuiCond_Always);
    ImGui::SetNextWindowSize(bar_size, ImGuiCond_Always);

    ImGuiWindowFlags flags = ImGuiWindowFlags_NoDecoration |
                            ImGuiWindowFlags_NoMove |
                            ImGuiWindowFlags_NoResize |
                            ImGuiWindowFlags_NoSavedSettings;

    ImGui::PushStyleColor(ImGuiCol_WindowBg, ImVec4(0.04f, 0.05f, 0.08f, 0.95f));
    ImGui::PushStyleColor(ImGuiCol_Border, ImVec4(0.18f, 0.24f, 0.36f, 0.90f));
    ImGui::PushStyleVar(ImGuiStyleVar_WindowBorderSize, 1.0f);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, 0.0f);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(16.0f, 8.0f));

    if (ImGui::Begin("##MinimalTopBar", nullptr, flags)) {
        // Brand Title
        ImGui::AlignTextToFramePadding();
        ImGui::TextColored(ImVec4(0.35f, 0.75f, 1.0f, 1.0f), "FIFA 22 MULTIPLAYER");

        // Steam Integration Tag
        if (SteamBridge::Instance().IsActive()) {
            ImGui::SameLine();
            ImGui::TextColored(ImVec4(0.3f, 0.35f, 0.45f, 1.0f), "|");
            ImGui::SameLine();
            ImGui::TextColored(
                ImVec4(0.40f, 0.70f, 1.0f, 1.0f),
                "[ Steam: %s (480) ]",
                SteamBridge::Instance().GetLocalPersonaName().c_str()
            );
        }

        ImGui::SameLine();
        ImGui::TextColored(ImVec4(0.3f, 0.35f, 0.45f, 1.0f), "|");

        // Active Mode indicator
        ImGui::SameLine();
        if (status.room_code.empty()) {
            ImGui::TextColored(ImVec4(0.30f, 0.85f, 0.60f, 1.0f), "[ Mode: Native Auto-Match ]");
        } else {
            ImGui::TextColored(ImVec4(1.0f, 0.80f, 0.30f, 1.0f), "[ Room Active: %s ]", status.room_code.c_str());
        }

        ImGui::SameLine();
        ImGui::TextColored(ImVec4(0.3f, 0.35f, 0.45f, 1.0f), "|");

        // Room Code Input
        ImGui::SameLine();
        ImGui::Text("Room Code:");

        ImGui::SameLine();
        ImGui::SetNextItemWidth(90.0f);
        if (ImGui::InputText("##RoomCodeInput", s_room_code_input, sizeof(s_room_code_input),
                            ImGuiInputTextFlags_CharsUppercase)) {
            // Capitalize input
            for (char* c = s_room_code_input; *c; ++c) {
                *c = static_cast<char>(toupper(*c));
            }
        }

        // Host Button
        ImGui::SameLine();
        if (ImGui::Button("Host")) {
            ControlClient::Instance().RequestHostRoom([](bool ok, const std::string& code) {
                if (ok) {
                    strncpy(s_room_code_input, code.c_str(), sizeof(s_room_code_input) - 1);
                    SteamBridge::Instance().SetRichPresenceStatus("Hosting Match Lobby", code);
                    SetFeedback("Room created: " + code + " (Copied to clipboard!)");
                } else {
                    SetFeedback("Failed to create room.");
                }
            });
        }

        // Join Button
        ImGui::SameLine();
        if (ImGui::Button("Join")) {
            std::string code_to_join = s_room_code_input;
            if (code_to_join.empty() && !status.room_code.empty()) {
                code_to_join = status.room_code;
            }
            if (!code_to_join.empty()) {
                ControlClient::Instance().RequestJoinRoom(code_to_join, [code_to_join](bool ok, const std::string&) {
                    if (ok) {
                        SteamBridge::Instance().SetRichPresenceStatus("Joined Match Lobby", code_to_join);
                        SetFeedback("Joined Room: " + code_to_join + " - enter Native Matchmaking!");
                    } else {
                        SetFeedback("Failed to join room.");
                    }
                });
            } else {
                SetFeedback("Please enter a 6-character room code.");
            }
        }

        // Copy Button
        ImGui::SameLine();
        if (ImGui::Button("Copy")) {
            std::string code = s_room_code_input;
            if (code.empty() && !status.room_code.empty()) {
                code = status.room_code;
            }
            if (!code.empty()) {
                ControlClient::CopyToClipboard(code);
                SetFeedback("Copied " + code + " to clipboard!");
            }
        }

        // Shift+Tab Invite Button (Steam Overlay)
        if (SteamBridge::Instance().IsActive()) {
            ImGui::SameLine();
            if (ImGui::Button("Shift+Tab Invite")) {
                SteamBridge::Instance().OpenOverlayInviteDialog();
                SetFeedback("Opening Steam Overlay Friend Invite Dialog...");
            }
        }

        // Feedback message
        if (!s_feedback_msg.empty()) {
            if (std::chrono::steady_clock::now() < s_feedback_expire) {
                ImGui::SameLine();
                ImGui::TextColored(ImVec4(0.20f, 0.90f, 0.40f, 1.0f), "%s", s_feedback_msg.c_str());
            } else {
                s_feedback_msg.clear();
            }
        }

        // Right-aligned hint: [ F1 to Hide | F2 Status ]
        float hint_width = 170.0f;
        ImGui::SameLine(viewport->WorkSize.x - hint_width - 16.0f);
        ImGui::TextColored(ImVec4(0.50f, 0.55f, 0.65f, 1.0f), "[ F1 Hide | F2 Pill ]");
    }
    ImGui::End();

    ImGui::PopStyleVar(3);
    ImGui::PopStyleColor(2);
}

static void RenderSideSelectGamerCards(const ServerStatus& status) {
    if (!IsStatusPillVisible()) return;

    const ImGuiViewport* viewport = ImGui::GetMainViewport();
    ImVec2 work_pos = viewport->WorkPos;
    ImVec2 work_size = viewport->WorkSize;

    NetcodeMetrics netcode = GetCurrentNetcodeMetrics();

    float card_width = 175.0f;
    float card_height = 56.0f;
    float spacing = 16.0f;
    float total_width = (card_width * 4.0f) + (spacing * 3.0f);
    float start_x = work_pos.x + (work_size.x - total_width) * 0.5f;
    // Position directly above FIFA 22's controller selection boxes (~54% screen height)
    float start_y = work_pos.y + (work_size.y * 0.54f);

    ImGuiWindowFlags flags = ImGuiWindowFlags_NoDecoration |
                            ImGuiWindowFlags_NoMove |
                            ImGuiWindowFlags_NoResize |
                            ImGuiWindowFlags_NoSavedSettings |
                            ImGuiWindowFlags_NoFocusOnAppearing |
                            ImGuiWindowFlags_NoNav |
                            ImGuiWindowFlags_NoInputs; // NEVER capture mouse/keyboard inputs!

    ImGui::PushStyleVar(ImGuiStyleVar_WindowRounding, 6.0f);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowBorderSize, 1.2f);
    ImGui::PushStyleVar(ImGuiStyleVar_WindowPadding, ImVec2(8.0f, 6.0f));

    for (int slot = 0; slot < 4; ++slot) {
        float x = start_x + (slot * (card_width + spacing));
        ImGui::SetNextWindowPos(ImVec2(x, start_y), ImGuiCond_Always);
        ImGui::SetNextWindowSize(ImVec2(card_width, card_height), ImGuiCond_Always);

        std::string win_id = "##GamerCardSlot" + std::to_string(slot);

        if (slot == 0) {
            // Slot 0: Host / Local Player
            ImGui::PushStyleColor(ImGuiCol_WindowBg, ImVec4(0.04f, 0.06f, 0.09f, 0.88f));
            ImGui::PushStyleColor(ImGuiCol_Border, ImVec4(0.20f, 0.55f, 0.90f, 0.85f));

            if (ImGui::Begin(win_id.c_str(), nullptr, flags)) {
                ImGui::TextColored(ImVec4(0.40f, 0.75f, 1.0f, 1.0f), "P1 [ HOST ]");
                std::string local_name = SteamBridge::Instance().IsActive()
                    ? SteamBridge::Instance().GetLocalPersonaName()
                    : "";
                if (local_name.empty() && !status.members.empty()) {
                    for (const auto& m : status.members) {
                        if (m.slot == 0 && !m.name.empty()) {
                            local_name = m.name;
                            break;
                        }
                    }
                }
                if (local_name.empty()) local_name = "Local Host";

                ImGui::TextColored(ImVec4(1.0f, 1.0f, 1.0f, 1.0f), "%s", local_name.c_str());
                ImGui::TextColored(ImVec4(0.30f, 0.85f, 0.45f, 1.0f), "● Ready");
            }
            ImGui::End();
            ImGui::PopStyleColor(2);
        } else {
            // Slots 1, 2, 3: Friends / Clients
            bool connected = IsRemotePeerConnected(slot);
            std::string guest_name = GetRemotePeerName(slot);

            // Also check if member exists in status.members from control server
            const RoomMember* member_info = nullptr;
            for (const auto& m : status.members) {
                if (m.slot == slot) {
                    member_info = &m;
                    break;
                }
            }

            if (member_info && member_info->connected) {
                connected = true;
                if (!member_info->name.empty() && member_info->name != "Friend") {
                    guest_name = member_info->name;
                }
            }

            ImVec4 border_col = connected
                ? ImVec4(0.18f, 0.85f, 0.45f, 0.85f)
                : ImVec4(0.25f, 0.30f, 0.40f, 0.50f);

            ImGui::PushStyleColor(ImGuiCol_WindowBg, ImVec4(0.04f, 0.06f, 0.09f, 0.88f));
            ImGui::PushStyleColor(ImGuiCol_Border, border_col);

            if (ImGui::Begin(win_id.c_str(), nullptr, flags)) {
                std::string slot_title = "P" + std::to_string(slot + 1) + " [ GUEST ]";
                ImGui::TextColored(ImVec4(0.95f, 0.80f, 0.30f, 1.0f), "%s", slot_title.c_str());

                if (guest_name.empty() || guest_name == "Friend") {
                    guest_name = "Player " + std::to_string(slot + 1);
                }
                ImGui::TextColored(ImVec4(1.0f, 1.0f, 1.0f, 1.0f), "%s", guest_name.c_str());

                if (connected) {
                    uint32_t ping = (member_info && member_info->ping_ms > 0)
                        ? member_info->ping_ms
                        : (netcode.ping_ms > 0 ? netcode.ping_ms : 18);
                    ImGui::TextColored(ImVec4(0.20f, 0.90f, 0.45f, 1.0f), "● Online (%ums)", ping);
                } else {
                    ImGui::TextColored(ImVec4(0.50f, 0.55f, 0.65f, 0.8f), "○ Waiting (F1)");
                }
            }
            ImGui::End();
            ImGui::PopStyleColor(2);
        }
    }

    ImGui::PopStyleVar(3);
}

void RenderMinimalOverlay() {
    ServerStatus status = ControlClient::Instance().GetStatus();

    // 1. Render Status Pill (F2 toggle, default visible)
    RenderStatusPill(status);

    // 2. Render Side-Select Gamer Cards HUD
    RenderSideSelectGamerCards(status);

    // 3. Render Minimal Top Bar (F1 toggle)
    RenderTopBar(status);
}

} // namespace f22

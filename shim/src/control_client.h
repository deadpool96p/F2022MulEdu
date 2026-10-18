#pragma once

#include <string>
#include <mutex>
#include <thread>
#include <atomic>
#include <functional>

#include <vector>

namespace f22 {

struct RoomMember {
    int slot = 0;
    std::string name;
    uint64_t steam_id = 0;
    std::string role; // "host" | "guest"
    int ping_ms = 18;
    bool connected = true;
};

struct ServerStatus {
    bool online = false;
    int active_players = 0;
    std::string room_code;
    int peer_ping_ms = 0;
    std::string transport_mode = "P2P"; // "P2P" | "RELAY" | "DISCONNECTED"
    std::string last_error;
    std::vector<RoomMember> members;
};

class ControlClient {
public:
    static ControlClient& Instance();

    void Start(const std::string& host, uint16_t port);
    void Stop();

    ServerStatus GetStatus();
    void RequestHostRoom(std::function<void(bool success, const std::string& room_code)> on_complete = nullptr);
    void RequestJoinRoom(const std::string& room_code, std::function<void(bool success, const std::string& message)> on_complete = nullptr);

    static void CopyToClipboard(const std::string& text);

private:
    ControlClient() = default;
    ~ControlClient();

    void WorkerLoop();
    bool FetchStatusSync(ServerStatus& out_status);
    bool PostHostRoomSync(std::string& out_code);
    bool PostJoinRoomSync(const std::string& room_code, std::string& out_status);

    std::string m_host = "127.0.0.1";
    uint16_t m_port = 42129;
    std::atomic<bool> m_running{false};
    std::thread m_worker_thread;

    std::mutex m_mutex;
    ServerStatus m_cached_status;
};

} // namespace f22

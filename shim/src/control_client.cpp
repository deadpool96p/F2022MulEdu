#include "control_client.h"
#include "f22_shim.h"
#include <windows.h>
#include <winhttp.h>
#include <sstream>
#include <iostream>
#include <algorithm>

namespace f22 {

// Helper to extract a JSON string field: "key": "value"
static std::string ExtractJsonString(const std::string& json, const std::string& key) {
    std::string search = "\"" + key + "\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return "";

    pos += search.length();
    // Skip whitespace
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t' || json[pos] == '\r' || json[pos] == '\n')) pos++;

    if (pos < json.length() && json[pos] == '\"') {
        pos++;
        size_t end = json.find('\"', pos);
        if (end != std::string::npos) {
            return json.substr(pos, end - pos);
        }
    }
    return "";
}

// Helper to extract a JSON integer field: "key": 123
static int ExtractJsonInt(const std::string& json, const std::string& key, int default_val = 0) {
    std::string search = "\"" + key + "\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return default_val;

    pos += search.length();
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t')) pos++;

    try {
        return std::stoi(json.substr(pos));
    } catch (...) {
        return default_val;
    }
}

// Helper to extract a JSON bool field: "key": true / false
static bool ExtractJsonBool(const std::string& json, const std::string& key, bool default_val = false) {
    std::string search = "\"" + key + "\":";
    size_t pos = json.find(search);
    if (pos == std::string::npos) return default_val;

    pos += search.length();
    while (pos < json.length() && (json[pos] == ' ' || json[pos] == '\t')) pos++;

    if (json.compare(pos, 4, "true") == 0) return true;
    if (json.compare(pos, 5, "false") == 0) return false;
    return default_val;
}

ControlClient& ControlClient::Instance() {
    static ControlClient s_instance;
    return s_instance;
}

ControlClient::~ControlClient() {
    Stop();
}

void ControlClient::Start(const std::string& host, uint16_t port) {
    if (m_running.exchange(true)) return;

    m_host = host;
    m_port = port;
    m_worker_thread = std::thread(&ControlClient::WorkerLoop, this);
}

void ControlClient::Stop() {
    if (!m_running.exchange(false)) return;

    if (m_worker_thread.joinable()) {
        m_worker_thread.join();
    }
}

ServerStatus ControlClient::GetStatus() {
    std::lock_guard<std::mutex> lock(m_mutex);
    return m_cached_status;
}

void ControlClient::CopyToClipboard(const std::string& text) {
    if (!OpenClipboard(nullptr)) return;
    EmptyClipboard();

    HGLOBAL hGlob = GlobalAlloc(GMEM_MOVEABLE, text.length() + 1);
    if (hGlob) {
        memcpy(GlobalLock(hGlob), text.c_str(), text.length() + 1);
        GlobalUnlock(hGlob);
        SetClipboardData(CF_TEXT, hGlob);
    }
    CloseClipboard();
}

void ControlClient::WorkerLoop() {
    while (m_running.load()) {
        ServerStatus status;
        if (FetchStatusSync(status)) {
            std::lock_guard<std::mutex> lock(m_mutex);
            m_cached_status = status;
        } else {
            std::lock_guard<std::mutex> lock(m_mutex);
            m_cached_status.online = false;
            m_cached_status.transport_mode = "DISCONNECTED";
        }

        // Sleep for 1.5 seconds between status polls
        for (int i = 0; i < 15 && m_running.load(); ++i) {
            std::this_thread::sleep_for(std::chrono::milliseconds(100));
        }
    }
}

bool ControlClient::FetchStatusSync(ServerStatus& out_status) {
    HINTERNET hSession = WinHttpOpen(L"FIFA22-Shim/1.0",
                                     WINHTTP_ACCESS_TYPE_NO_PROXY,
                                     WINHTTP_NO_PROXY_NAME,
                                     WINHTTP_NO_PROXY_BYPASS, 0);
    if (!hSession) return false;

    std::wstring wHost(m_host.begin(), m_host.end());
    HINTERNET hConnect = WinHttpConnect(hSession, wHost.c_str(), m_port, 0);
    if (!hConnect) {
        WinHttpCloseHandle(hSession);
        return false;
    }

    HINTERNET hRequest = WinHttpOpenRequest(hConnect, L"GET", L"/api/v1/status",
                                           nullptr, WINHTTP_NO_REFERER,
                                           WINHTTP_DEFAULT_ACCEPT_TYPES, 0);
    if (!hRequest) {
        WinHttpCloseHandle(hConnect);
        WinHttpCloseHandle(hSession);
        return false;
    }

    // Set timeout to 1 second so we don't hang if server is temporarily unreachable
    WinHttpSetTimeouts(hRequest, 1000, 1000, 1000, 1000);

    bool success = false;
    if (WinHttpSendRequest(hRequest, WINHTTP_NO_ADDITIONAL_HEADERS, 0,
                           WINHTTP_NO_REQUEST_DATA, 0, 0, 0) &&
        WinHttpReceiveResponse(hRequest, nullptr)) {

        DWORD statusCode = 0;
        DWORD size = sizeof(statusCode);
        WinHttpQueryHeaders(hRequest, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                            WINHTTP_HEADER_NAME_BY_INDEX, &statusCode, &size, WINHTTP_NO_HEADER_INDEX);

        if (statusCode == 200) {
            std::string response;
            DWORD bytesAvailable = 0;
            while (WinHttpQueryDataAvailable(hRequest, &bytesAvailable) && bytesAvailable > 0) {
                std::vector<char> buffer(bytesAvailable + 1);
                DWORD bytesRead = 0;
                if (WinHttpReadData(hRequest, buffer.data(), bytesAvailable, &bytesRead)) {
                    response.append(buffer.data(), bytesRead);
                }
            }

            out_status.online = ExtractJsonBool(response, "online", false) || (ExtractJsonString(response, "status") == "online");
            out_status.active_players = ExtractJsonInt(response, "active_players", ExtractJsonInt(response, "players_online", 0));
            out_status.room_code = ExtractJsonString(response, "room_code");
            if (out_status.room_code.empty()) {
                out_status.room_code = ExtractJsonString(response, "current_room");
            }
            out_status.peer_ping_ms = ExtractJsonInt(response, "peer_ping_ms", ExtractJsonInt(response, "ping_ms", 18));
            out_status.transport_mode = ExtractJsonString(response, "transport_mode");
            if (out_status.transport_mode.empty()) {
                out_status.transport_mode = "P2P";
            }
            success = true;
        }
    }

    WinHttpCloseHandle(hRequest);
    WinHttpCloseHandle(hConnect);
    WinHttpCloseHandle(hSession);
    return success;
}

bool ControlClient::PostHostRoomSync(std::string& out_code) {
    HINTERNET hSession = WinHttpOpen(L"FIFA22-Shim/1.0",
                                     WINHTTP_ACCESS_TYPE_NO_PROXY,
                                     WINHTTP_NO_PROXY_NAME,
                                     WINHTTP_NO_PROXY_BYPASS, 0);
    if (!hSession) return false;

    std::wstring wHost(m_host.begin(), m_host.end());
    HINTERNET hConnect = WinHttpConnect(hSession, wHost.c_str(), m_port, 0);
    if (!hConnect) {
        WinHttpCloseHandle(hSession);
        return false;
    }

    HINTERNET hRequest = WinHttpOpenRequest(hConnect, L"POST", L"/api/v1/rooms/host",
                                           nullptr, WINHTTP_NO_REFERER,
                                           WINHTTP_DEFAULT_ACCEPT_TYPES, 0);
    if (!hRequest) {
        WinHttpCloseHandle(hConnect);
        WinHttpCloseHandle(hSession);
        return false;
    }

    WinHttpSetTimeouts(hRequest, 2000, 2000, 2000, 2000);

    LPCWSTR headers = L"Content-Type: application/json\r\n";
    bool success = false;
    if (WinHttpSendRequest(hRequest, headers, (DWORD)-1L,
                           WINHTTP_NO_REQUEST_DATA, 0, 0, 0) &&
        WinHttpReceiveResponse(hRequest, nullptr)) {

        DWORD statusCode = 0;
        DWORD size = sizeof(statusCode);
        WinHttpQueryHeaders(hRequest, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                            WINHTTP_HEADER_NAME_BY_INDEX, &statusCode, &size, WINHTTP_NO_HEADER_INDEX);

        if (statusCode == 200) {
            std::string response;
            DWORD bytesAvailable = 0;
            while (WinHttpQueryDataAvailable(hRequest, &bytesAvailable) && bytesAvailable > 0) {
                std::vector<char> buffer(bytesAvailable + 1);
                DWORD bytesRead = 0;
                if (WinHttpReadData(hRequest, buffer.data(), bytesAvailable, &bytesRead)) {
                    response.append(buffer.data(), bytesRead);
                }
            }

            out_code = ExtractJsonString(response, "room_code");
            if (!out_code.empty()) {
                success = true;
            }
        }
    }

    WinHttpCloseHandle(hRequest);
    WinHttpCloseHandle(hConnect);
    WinHttpCloseHandle(hSession);
    return success;
}

bool ControlClient::PostJoinRoomSync(const std::string& room_code, std::string& out_status) {
    HINTERNET hSession = WinHttpOpen(L"FIFA22-Shim/1.0",
                                     WINHTTP_ACCESS_TYPE_NO_PROXY,
                                     WINHTTP_NO_PROXY_NAME,
                                     WINHTTP_NO_PROXY_BYPASS, 0);
    if (!hSession) return false;

    std::wstring wHost(m_host.begin(), m_host.end());
    HINTERNET hConnect = WinHttpConnect(hSession, wHost.c_str(), m_port, 0);
    if (!hConnect) {
        WinHttpCloseHandle(hSession);
        return false;
    }

    HINTERNET hRequest = WinHttpOpenRequest(hConnect, L"POST", L"/api/v1/rooms/join",
                                           nullptr, WINHTTP_NO_REFERER,
                                           WINHTTP_DEFAULT_ACCEPT_TYPES, 0);
    if (!hRequest) {
        WinHttpCloseHandle(hConnect);
        WinHttpCloseHandle(hSession);
        return false;
    }

    WinHttpSetTimeouts(hRequest, 2000, 2000, 2000, 2000);

    LPCWSTR headers = L"Content-Type: application/json\r\n";
    std::string body = "{\"room_code\":\"" + room_code + "\"}";

    bool success = false;
    if (WinHttpSendRequest(hRequest, headers, (DWORD)-1L,
                           (LPVOID)body.data(), (DWORD)body.length(), (DWORD)body.length(), 0) &&
        WinHttpReceiveResponse(hRequest, nullptr)) {

        DWORD statusCode = 0;
        DWORD size = sizeof(statusCode);
        WinHttpQueryHeaders(hRequest, WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                            WINHTTP_HEADER_NAME_BY_INDEX, &statusCode, &size, WINHTTP_NO_HEADER_INDEX);

        if (statusCode == 200) {
            std::string response;
            DWORD bytesAvailable = 0;
            while (WinHttpQueryDataAvailable(hRequest, &bytesAvailable) && bytesAvailable > 0) {
                std::vector<char> buffer(bytesAvailable + 1);
                DWORD bytesRead = 0;
                if (WinHttpReadData(hRequest, buffer.data(), bytesAvailable, &bytesRead)) {
                    response.append(buffer.data(), bytesRead);
                }
            }

            out_status = ExtractJsonString(response, "status");
            success = true;
        }
    }

    WinHttpCloseHandle(hRequest);
    WinHttpCloseHandle(hConnect);
    WinHttpCloseHandle(hSession);
    return success;
}

void ControlClient::RequestHostRoom(std::function<void(bool success, const std::string& room_code)> on_complete) {
    std::thread([this, on_complete]() {
        std::string code;
        bool ok = PostHostRoomSync(code);
        if (ok) {
            std::lock_guard<std::mutex> lock(m_mutex);
            m_cached_status.room_code = code;
            CopyToClipboard(code);
        }
        if (on_complete) {
            on_complete(ok, code);
        }
    }).detach();
}

void ControlClient::RequestJoinRoom(const std::string& room_code, std::function<void(bool success, const std::string& message)> on_complete) {
    std::thread([this, room_code, on_complete]() {
        std::string status;
        bool ok = PostJoinRoomSync(room_code, status);
        if (ok) {
            std::lock_guard<std::mutex> lock(m_mutex);
            m_cached_status.room_code = room_code;
        }
        if (on_complete) {
            on_complete(ok, status);
        }
    }).detach();
}

} // namespace f22

#pragma once
#include <string>
#include <fstream>
#include <algorithm>
#include <cctype>
#include <filesystem>

struct SeamlessConfig {
    std::string relay_host = "127.0.0.1";
    uint16_t relay_port = 27015;
    std::string session_password = "fifa_coop_room";
    std::string role = "host"; // "host" or "guest"
    bool prefer_p2p = true;
    bool enable_overlay = true;
    std::string log_level = "info";
};

inline std::string Trim(const std::string& str) {
    size_t first = str.find_first_not_of(" \t\r\n");
    if (first == std::string::npos) return "";
    size_t last = str.find_last_not_of(" \t\r\n");
    return str.substr(first, (last - first + 1));
}

inline SeamlessConfig LoadSeamlessConfig(const std::wstring& ini_path) {
    SeamlessConfig cfg;
    std::filesystem::path p(ini_path);
    std::ifstream file(p);
    if (!file.is_open()) {
        return cfg;
    }

    std::string line;
    while (std::getline(file, line)) {
        line = Trim(line);
        if (line.empty() || line[0] == ';' || line[0] == '#') continue;
        if (line.front() == '[' && line.back() == ']') continue;

        size_t eq = line.find('=');
        if (eq == std::string::npos) continue;

        std::string key = Trim(line.substr(0, eq));
        std::string val = Trim(line.substr(eq + 1));

        // Strip quotes if present
        if (val.size() >= 2 && val.front() == '"' && val.back() == '"') {
            val = val.substr(1, val.size() - 2);
        }

        if (key == "relay_host" || key == "server_ip") {
            cfg.relay_host = val;
        } else if (key == "relay_port" || key == "server_port") {
            cfg.relay_port = static_cast<uint16_t>(std::stoi(val));
        } else if (key == "session_password" || key == "password") {
            cfg.session_password = val;
        } else if (key == "role") {
            cfg.role = val;
        } else if (key == "prefer_p2p") {
            cfg.prefer_p2p = (val == "true" || val == "1");
        } else if (key == "enable_overlay" || key == "overlay") {
            cfg.enable_overlay = (val == "true" || val == "1");
        } else if (key == "log_level") {
            cfg.log_level = val;
        }
    }
    return cfg;
}

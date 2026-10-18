#include "f22_shim.h"
#include <fstream>
#include <sstream>
#include <algorithm>

namespace f22 {

static Config g_config;

Config& GetConfig() {
    return g_config;
}

static std::string Trim(const std::string& str) {
    auto first = str.find_first_not_of(" \t\r\n");
    if (first == std::string::npos) return "";
    auto last = str.find_last_not_of(" \t\r\n");
    return str.substr(first, (last - first + 1));
}

void LoadConfig() {
    std::ifstream file("f22_emu.ini");
    if (!file.is_open()) {
        LogInfo("No f22_emu.ini found; using default emulator loopback configuration.");
        return;
    }

    std::string line;
    std::string current_section;

    while (std::getline(file, line)) {
        line = Trim(line);
        if (line.empty() || line[0] == ';' || line[0] == '#') {
            continue;
        }

        if (line.front() == '[' && line.back() == ']') {
            current_section = line.substr(1, line.size() - 2);
            continue;
        }

        auto eq_pos = line.find('=');
        if (eq_pos == std::string::npos) continue;

        std::string key = Trim(line.substr(0, eq_pos));
        std::string val = Trim(line.substr(eq_pos + 1));

        if (current_section == "Network") {
            if (key == "EmulatorHost") {
                g_config.emulator_host = val;
            } else if (key == "RedirectorPort") {
                g_config.redirector_port = static_cast<uint16_t>(std::stoi(val));
            } else if (key == "BlazePort") {
                g_config.blaze_port = static_cast<uint16_t>(std::stoi(val));
            } else if (key == "TelemetryPort") {
                g_config.telemetry_port = static_cast<uint16_t>(std::stoi(val));
            } else if (key == "RerouteAllEA") {
                g_config.reroute_all_ea = (val == "true" || val == "1");
            } else if (key == "ForcePlainTCP") {
                g_config.force_plain_tcp = (val == "true" || val == "1");
            } else if (key == "LogSockets") {
                g_config.log_sockets = (val == "true" || val == "1");
            }
        } else if (current_section == "SSL") {
            if (key == "BypassCertValidation") {
                g_config.bypass_cert_validation = (val == "true" || val == "1");
            }
        }
    }

    std::ostringstream ss;
    ss << "Config loaded: Host=" << g_config.emulator_host
       << ", RedirectorPort=" << g_config.redirector_port
       << ", BlazePort=" << g_config.blaze_port
       << ", TelemetryPort=" << g_config.telemetry_port
       << ", BypassCert=" << (g_config.bypass_cert_validation ? "true" : "false");
    LogInfo(ss.str());
}

} // namespace f22

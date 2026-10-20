#include "f22_shim.h"
#include <fstream>
#include <mutex>
#include <chrono>
#include <iomanip>
#include <sstream>

namespace f22 {

static std::mutex g_log_mutex;
static std::ofstream g_log_file;
static std::ofstream g_appdata_shim_log;
static bool g_log_initialized = false;
static std::string g_log_dir = "";

void SetLogDirectory(const std::string& dir) {
    std::lock_guard<std::mutex> lock(g_log_mutex);
    g_log_dir = dir;
    if (g_log_file.is_open()) {
        g_log_file.close();
    }
    if (g_appdata_shim_log.is_open()) {
        g_appdata_shim_log.close();
    }
    g_log_initialized = false;
}

static void EnsureLogFile() {
    if (!g_log_initialized) {
        if (!g_log_dir.empty()) {
            std::string full_path = g_log_dir;
            if (full_path.back() != '\\' && full_path.back() != '/') {
                full_path += "\\";
            }
            full_path += "f22_shim.log";
            g_log_file.open(full_path, std::ios::out | std::ios::app);
        }

        if (!g_log_file.is_open()) {
            g_log_file.open("f22_shim.log", std::ios::out | std::ios::app);
        }

        if (!g_log_file.is_open()) {
            CreateDirectoryA("C:\\Temp", nullptr);
            g_log_file.open("C:\\Temp\\f22_shim.log", std::ios::out | std::ios::app);
        }

        // Also open %LOCALAPPDATA%\F2022MulEdu\shim.log
        char local_appdata[MAX_PATH] = {0};
        if (GetEnvironmentVariableA("LOCALAPPDATA", local_appdata, MAX_PATH) > 0) {
            std::string dir = std::string(local_appdata) + "\\F2022MulEdu";
            CreateDirectoryA(dir.c_str(), nullptr);
            std::string path = dir + "\\shim.log";
            g_appdata_shim_log.open(path, std::ios::out | std::ios::app);
        }

        g_log_initialized = true;
    }
}

void Log(std::string_view level, std::string_view message) {
    auto now = std::chrono::system_clock::now();
    auto in_time_t = std::chrono::system_clock::to_time_t(now);
    auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(now.time_since_epoch()) % 1000;

    std::tm time_info{};
    localtime_s(&time_info, &in_time_t);

    std::ostringstream ss;
    ss << "[" << std::put_time(&time_info, "%Y-%m-%d %H:%M:%S")
       << "." << std::setfill('0') << std::setw(3) << ms.count() << "] "
       << "[" << std::setw(5) << level << "] "
       << "[TID " << GetCurrentThreadId() << "] "
       << message << "\n";

    std::string formatted = ss.str();

    std::lock_guard<std::mutex> lock(g_log_mutex);
    EnsureLogFile();
    if (g_log_file.is_open()) {
        g_log_file << formatted;
        g_log_file.flush();
    }
    if (g_appdata_shim_log.is_open()) {
        g_appdata_shim_log << formatted;
        g_appdata_shim_log.flush();
    }
    OutputDebugStringA(formatted.c_str());
}

void LogInfo(std::string_view message) {
    Log("INFO", message);
}

void LogWarn(std::string_view message) {
    Log("WARN", message);
}

void LogError(std::string_view message) {
    Log("ERROR", message);
}

} // namespace f22

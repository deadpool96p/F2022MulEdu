#include "f22_shim.h"
#include <fstream>
#include <mutex>
#include <chrono>
#include <iomanip>
#include <sstream>
#include <thread>
#include <queue>
#include <condition_variable>
#include <atomic>

namespace f22 {

static std::mutex g_log_mutex;
static std::queue<std::string> g_log_queue;
static std::condition_variable g_log_cv;
static std::atomic<bool> g_log_running{false};
static std::thread g_log_worker;
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

static void LoggerWorkerThread() {
    while (g_log_running.load(std::memory_order_relaxed)) {
        std::string msg;
        {
            std::unique_lock<std::mutex> lock(g_log_mutex);
            g_log_cv.wait(lock, [] {
                return !g_log_queue.empty() || !g_log_running.load(std::memory_order_relaxed);
            });
            if (g_log_queue.empty()) {
                if (!g_log_running.load(std::memory_order_relaxed)) break;
                continue;
            }
            msg = std::move(g_log_queue.front());
            g_log_queue.pop();
        }

        EnsureLogFile();
        if (g_log_file.is_open()) {
            g_log_file << msg;
            g_log_file.flush();
        }
        if (g_appdata_shim_log.is_open()) {
            g_appdata_shim_log << msg;
            g_appdata_shim_log.flush();
        }
        OutputDebugStringA(msg.c_str());
    }
}

static void StartLoggerWorkerIfNeeded() {
    static std::once_flag s_once;
    std::call_once(s_once, [] {
        g_log_running.store(true, std::memory_order_relaxed);
        g_log_worker = std::thread(LoggerWorkerThread);
        g_log_worker.detach();
    });
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

    {
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

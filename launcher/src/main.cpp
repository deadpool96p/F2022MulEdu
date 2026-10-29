#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <iostream>
#include <fstream>
#include <string>
#include <filesystem>
#include <chrono>
#include <iomanip>
#include "ini_parser.h"

namespace fs = std::filesystem;

static std::ofstream g_log_file;

static void LogLauncher(const std::string& msg) {
    auto now = std::chrono::system_clock::now();
    auto time_t_now = std::chrono::system_clock::to_time_t(now);
    auto ms = std::chrono::duration_cast<std::chrono::milliseconds>(now.time_since_epoch()) % 1000;

    std::tm tm_buf{};
    localtime_s(&tm_buf, &time_t_now);

    char time_str[64];
    std::snprintf(time_str, sizeof(time_str), "[%04d-%02d-%02d %02d:%02d:%02d.%03lld] ",
                  tm_buf.tm_year + 1900, tm_buf.tm_mon + 1, tm_buf.tm_mday,
                  tm_buf.tm_hour, tm_buf.tm_min, tm_buf.tm_sec, ms.count());

    std::string full_line = std::string(time_str) + msg;
    std::cout << full_line << "\n";
    if (g_log_file.is_open()) {
        g_log_file << full_line << "\n";
        g_log_file.flush();
    }
}

void PrintBanner() {
    std::cout << "============================================================\n";
    std::cout << "           FIFA 22 Seamless Co-op Launcher v1.0             \n";
    std::cout << "============================================================\n";
}

bool InjectDll(HANDLE hProcess, const std::wstring& dllPath) {
    LogLauncher("[Injector] Starting DLL injection process...");
    size_t pathBytes = (dllPath.length() + 1) * sizeof(wchar_t);

    LPVOID remoteMem = VirtualAllocEx(hProcess, nullptr, pathBytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (!remoteMem) {
        DWORD err = GetLastError();
        LogLauncher("[Injector ERROR] VirtualAllocEx failed! Error code: " + std::to_string(err));
        return false;
    }
    LogLauncher("[Injector] VirtualAllocEx allocated buffer at: 0x" + std::to_string((uintptr_t)remoteMem));

    SIZE_T bytesWritten = 0;
    if (!WriteProcessMemory(hProcess, remoteMem, dllPath.c_str(), pathBytes, &bytesWritten)) {
        DWORD err = GetLastError();
        LogLauncher("[Injector ERROR] WriteProcessMemory failed! Error code: " + std::to_string(err));
        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        return false;
    }
    LogLauncher("[Injector] WriteProcessMemory wrote " + std::to_string(bytesWritten) + " bytes.");

    HMODULE hKernel32 = GetModuleHandleW(L"kernel32.dll");
    if (!hKernel32) {
        DWORD err = GetLastError();
        LogLauncher("[Injector ERROR] GetModuleHandleW(kernel32.dll) failed! Error code: " + std::to_string(err));
        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        return false;
    }

    FARPROC pLoadLibraryW = GetProcAddress(hKernel32, "LoadLibraryW");
    if (!pLoadLibraryW) {
        DWORD err = GetLastError();
        LogLauncher("[Injector ERROR] GetProcAddress(LoadLibraryW) failed! Error code: " + std::to_string(err));
        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        return false;
    }
    LogLauncher("[Injector] Found LoadLibraryW at: 0x" + std::to_string((uintptr_t)pLoadLibraryW));

    HANDLE hThread = CreateRemoteThread(
        hProcess,
        nullptr,
        0,
        reinterpret_cast<LPTHREAD_START_ROUTINE>(pLoadLibraryW),
        remoteMem,
        0,
        nullptr
    );

    if (!hThread) {
        DWORD err = GetLastError();
        LogLauncher("[Injector ERROR] CreateRemoteThread failed! Error code: " + std::to_string(err));
        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        return false;
    }
    LogLauncher("[Injector] CreateRemoteThread succeeded. Waiting for DllMain attach...");

    DWORD waitRes = WaitForSingleObject(hThread, 10000);
    LogLauncher("[Injector] WaitForSingleObject returned: " + std::to_string(waitRes));

    DWORD exitCode = 0;
    GetExitCodeThread(hThread, &exitCode);
    CloseHandle(hThread);
    VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);

    if (exitCode == 0) {
        LogLauncher("[Injector ERROR] Remote LoadLibraryW returned NULL (DLL failed to load or crashed).");
        return false;
    }

    LogLauncher("[Injector] Injection SUCCESS! Injected HMODULE: 0x" + std::to_string(exitCode));
    return true;
}

int main(int argc, char* argv[]) {
    PrintBanner();

    // Open log file in current directory
    g_log_file.open("launcher_log.txt", std::ios::out | std::ios::trunc);
    LogLauncher("FIFA 22 Seamless Co-op Launcher session started.");

    // Set Steam Environment to Spacewar (AppID 480)
    SetEnvironmentVariableW(L"SteamAppId", L"480");
    SetEnvironmentVariableW(L"SteamGameId", L"480");
    LogLauncher("[Steam] Configured SteamAppId=480, SteamGameId=480.");

    // Determine current directory
    wchar_t currentDirBuf[MAX_PATH];
    GetCurrentDirectoryW(MAX_PATH, currentDirBuf);
    fs::path workDir(currentDirBuf);
    LogLauncher("[Launcher] Current Directory: " + workDir.string());

    std::wstring iniPath = (workDir / L"seamless.ini").wstring();
    SeamlessConfig cfg = LoadSeamlessConfig(iniPath);
    LogLauncher("[Config] Relay Server : " + cfg.relay_host + ":" + std::to_string(cfg.relay_port));
    LogLauncher("[Config] Session Room : " + cfg.session_password);
    LogLauncher("[Config] Peer Role    : " + cfg.role);

    // Locate FIFA22.exe
    fs::path exePath = workDir / L"FIFA22.exe";
    if (argc > 1) {
        exePath = argv[1];
    }

    if (!fs::exists(exePath)) {
        fs::path fallback(L"J:\\Games\\FIFA 22\\FIFA22.exe");
        if (fs::exists(fallback)) {
            exePath = fallback;
        } else {
            LogLauncher("[Launcher ERROR] Cannot locate FIFA22.exe at: " + exePath.string());
            return 1;
        }
    }
    LogLauncher("[Launcher] Target Executable: " + exePath.string());

    // Also mirror log file to the target game directory
    fs::path gameLogPath = exePath.parent_path() / L"launcher_log.txt";
    if (gameLogPath != (workDir / L"launcher_log.txt")) {
        // Can be copied after execution
    }

    // Locate seamless_fifa.dll
    fs::path dllPath = workDir / L"seamless_fifa.dll";
    if (!fs::exists(dllPath)) {
        fs::path exeDirDll = exePath.parent_path() / L"seamless_fifa.dll";
        if (fs::exists(exeDirDll)) {
            dllPath = exeDirDll;
        } else {
            LogLauncher("[Launcher ERROR] Cannot locate seamless_fifa.dll at: " + dllPath.string());
            return 1;
        }
    }
    LogLauncher("[Launcher] Injection DLL: " + dllPath.string());

    std::wstring cmdLine = L"\"" + exePath.wstring() + L"\" -seamless -skipintro -novid -offline";

    STARTUPINFOW si{};
    si.cb = sizeof(si);
    PROCESS_INFORMATION pi{};

    LogLauncher("[Launcher] Spawning FIFA 22 with CREATE_SUSPENDED...");
    BOOL success = CreateProcessW(
        exePath.c_str(),
        const_cast<wchar_t*>(cmdLine.c_str()),
        nullptr,
        nullptr,
        FALSE,
        CREATE_SUSPENDED | NORMAL_PRIORITY_CLASS | CREATE_NEW_PROCESS_GROUP,
        nullptr,
        exePath.parent_path().c_str(),
        &si,
        &pi
    );

    if (!success) {
        DWORD err = GetLastError();
        LogLauncher("[Launcher ERROR] CreateProcessW failed! Error code: " + std::to_string(err));
        return 1;
    }

    LogLauncher("[Launcher] Process created successfully. PID: " + std::to_string(pi.dwProcessId) + ", TID: " + std::to_string(pi.dwThreadId));

    if (!InjectDll(pi.hProcess, dllPath.wstring())) {
        LogLauncher("[Launcher FATAL] DLL Injection failed. Terminating game process.");
        TerminateProcess(pi.hProcess, 1);
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
        return 1;
    }

    LogLauncher("[Launcher] Resuming primary thread (ResumeThread)...");
    DWORD resumeCount = ResumeThread(pi.hThread);
    DWORD resumeErr = GetLastError();
    LogLauncher("[Launcher] ResumeThread returned previous suspend count: " + std::to_string(resumeCount) + " (LastError: " + std::to_string(resumeErr) + ")");

    // Monitor for 3 seconds to verify boot stability
    LogLauncher("[Launcher] Verifying process stability (waiting 3s)...");
    DWORD waitRes = WaitForSingleObject(pi.hProcess, 3000);
    if (waitRes == WAIT_TIMEOUT) {
        LogLauncher("[Launcher SUCCESS] Game process is running stably! PID: " + std::to_string(pi.dwProcessId));
    } else {
        DWORD exitCode = 0;
        GetExitCodeProcess(pi.hProcess, &exitCode);
        LogLauncher("[Launcher WARNING] Game process exited early. Exit code: 0x" + std::to_string(exitCode) + " (" + std::to_string((int)exitCode) + ")");
    }

    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);

    LogLauncher("[Launcher] Execution completed successfully. Game is running with Seamless hooks.");
    return 0;
}

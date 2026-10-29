#define WIN32_LEAN_AND_MEAN
#include <windows.h>
#include <iostream>
#include <string>
#include <filesystem>
#include "ini_parser.h"

namespace fs = std::filesystem;

void PrintBanner() {
    std::cout << "============================================================\n";
    std::cout << "           FIFA 22 Seamless Co-op Launcher v1.0             \n";
    std::cout << "============================================================\n";
}

bool InjectDll(HANDLE hProcess, const std::wstring& dllPath) {
    size_t pathBytes = (dllPath.length() + 1) * sizeof(wchar_t);
    LPVOID remoteMem = VirtualAllocEx(hProcess, nullptr, pathBytes, MEM_COMMIT | MEM_RESERVE, PAGE_READWRITE);
    if (!remoteMem) {
        std::cerr << "[Launcher ERROR] VirtualAllocEx failed with error " << GetLastError() << "\n";
        return false;
    }

    if (!WriteProcessMemory(hProcess, remoteMem, dllPath.c_str(), pathBytes, nullptr)) {
        std::cerr << "[Launcher ERROR] WriteProcessMemory failed with error " << GetLastError() << "\n";
        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        return false;
    }

    HMODULE hKernel32 = GetModuleHandleW(L"kernel32.dll");
    if (!hKernel32) {
        std::cerr << "[Launcher ERROR] GetModuleHandleW(kernel32.dll) failed\n";
        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        return false;
    }

    FARPROC pLoadLibraryW = GetProcAddress(hKernel32, "LoadLibraryW");
    if (!pLoadLibraryW) {
        std::cerr << "[Launcher ERROR] GetProcAddress(LoadLibraryW) failed\n";
        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        return false;
    }

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
        std::cerr << "[Launcher ERROR] CreateRemoteThread failed with error " << GetLastError() << "\n";
        VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);
        return false;
    }

    std::cout << "[Launcher] Remote injection thread created. Awaiting DLL initialization...\n";
    WaitForSingleObject(hThread, 8000);

    DWORD exitCode = 0;
    GetExitCodeThread(hThread, &exitCode);
    CloseHandle(hThread);
    VirtualFreeEx(hProcess, remoteMem, 0, MEM_RELEASE);

    if (exitCode == 0) {
        std::cerr << "[Launcher ERROR] Remote LoadLibraryW returned NULL (DLL failed to load).\n";
        return false;
    }

    std::cout << "[Launcher] Injection successful! Injected HMODULE: 0x" << std::hex << exitCode << std::dec << "\n";
    return true;
}

int main(int argc, char* argv[]) {
    PrintBanner();

    // Set Steam Environment to Spacewar (AppID 480) for overlay & friend invitations
    SetEnvironmentVariableW(L"SteamAppId", L"480");
    SetEnvironmentVariableW(L"SteamGameId", L"480");

    // Determine current directory
    wchar_t currentDirBuf[MAX_PATH];
    GetCurrentDirectoryW(MAX_PATH, currentDirBuf);
    fs::path workDir(currentDirBuf);

    std::wstring iniPath = (workDir / L"seamless.ini").wstring();
    SeamlessConfig cfg = LoadSeamlessConfig(iniPath);

    std::cout << "[Config] Relay Server : " << cfg.relay_host << ":" << cfg.relay_port << "\n";
    std::cout << "[Config] Session Room : " << cfg.session_password << "\n";
    std::cout << "[Config] Peer Role    : " << cfg.role << "\n";
    std::cout << "[Config] In-Game HUD  : " << (cfg.enable_overlay ? "ENABLED" : "DISABLED") << "\n";
    std::cout << "[Steam]  AppID        : 480 (Spacewar P2P & Invites)\n";
    std::cout << "------------------------------------------------------------\n";

    // Locate FIFA22.exe
    fs::path exePath = workDir / L"FIFA22.exe";
    if (argc > 1) {
        exePath = argv[1];
    }

    if (!fs::exists(exePath)) {
        // Fallback to default J:\Games\FIFA 22\FIFA22.exe if local not found
        fs::path fallback(L"J:\\Games\\FIFA 22\\FIFA22.exe");
        if (fs::exists(fallback)) {
            exePath = fallback;
        } else {
            std::cerr << "[Launcher ERROR] Cannot locate FIFA22.exe at: " << exePath.string() << "\n";
            std::cerr << "Place FIFA22_Seamless.exe in your FIFA 22 directory or pass the path as argument.\n";
            return 1;
        }
    }

    // Locate seamless_fifa.dll
    fs::path dllPath = workDir / L"seamless_fifa.dll";
    if (!fs::exists(dllPath)) {
        // Check exe directory
        fs::path exeDirDll = exePath.parent_path() / L"seamless_fifa.dll";
        if (fs::exists(exeDirDll)) {
            dllPath = exeDirDll;
        } else {
            std::cerr << "[Launcher ERROR] Cannot locate seamless_fifa.dll at: " << dllPath.string() << "\n";
            return 1;
        }
    }

    std::wcout << L"[Launcher] Target Binary : " << exePath.wstring() << L"\n";
    std::wcout << L"[Launcher] Injection DLL : " << dllPath.wstring() << L"\n";

    std::wstring cmdLine = L"\"" + exePath.wstring() + L"\" -seamless -skipintro -novid -offline";

    STARTUPINFOW si{};
    si.cb = sizeof(si);
    PROCESS_INFORMATION pi{};

    std::cout << "[Launcher] Spawning FIFA 22 in suspended state...\n";
    BOOL success = CreateProcessW(
        exePath.c_str(),
        const_cast<wchar_t*>(cmdLine.c_str()),
        nullptr,
        nullptr,
        FALSE,
        CREATE_SUSPENDED | NORMAL_PRIORITY_CLASS,
        nullptr,
        exePath.parent_path().c_str(),
        &si,
        &pi
    );

    if (!success) {
        std::cerr << "[Launcher ERROR] CreateProcessW failed with error code: " << GetLastError() << "\n";
        return 1;
    }

    std::cout << "[Launcher] Process created (PID: " << pi.dwProcessId << "). Injecting hooks...\n";

    if (!InjectDll(pi.hProcess, dllPath.wstring())) {
        std::cerr << "[Launcher ERROR] Injection failed. Terminating process.\n";
        TerminateProcess(pi.hProcess, 1);
        CloseHandle(pi.hThread);
        CloseHandle(pi.hProcess);
        return 1;
    }

    std::cout << "[Launcher] Resuming main game thread. Enjoy Seamless Co-op!\n";
    ResumeThread(pi.hThread);

    CloseHandle(pi.hThread);
    CloseHandle(pi.hProcess);
    return 0;
}

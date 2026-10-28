#include "f22_shim.h"
#include "hook_guard.h"
#include <MinHook.h>
#include <bcrypt.h>
#include <psapi.h>
#include <mutex>
#include <set>
#include <sstream>
#include <vector>
#include <wincrypt.h>
#include <windows.h>
#include <atomic>
#include <thread>
#include <algorithm>
#include <cctype>

// LoadLibrary hook type declarations (must precede any use)
typedef HMODULE (WINAPI *PFN_LoadLibraryW)(LPCWSTR lpLibFileName);
typedef HMODULE (WINAPI *PFN_LoadLibraryExW)(LPCWSTR lpLibFileName, HANDLE hFile, DWORD dwFlags);
typedef HMODULE (WINAPI *PFN_LoadLibraryA)(LPCSTR lpLibFileName);
typedef HMODULE (WINAPI *PFN_LoadLibraryExA)(LPCSTR lpLibFileName, HANDLE hFile, DWORD dwFlags);
#include <cstring>

  static PFN_LoadLibraryW g_orig_LoadLibraryW = nullptr;
  static PFN_LoadLibraryExW g_orig_LoadLibraryExW = nullptr;
  static PFN_LoadLibraryA g_orig_LoadLibraryA = nullptr;
  static PFN_LoadLibraryExA g_orig_LoadLibraryExA = nullptr;

  namespace f22 {

  static bool BinaryPatchBoringSSLVerify();
  static bool PatchNetworkStateAndErrorHandler();

  // CryptoAPI Hooks
  using PFN_CertVerifyCertificateChainPolicy =
      BOOL(WINAPI *)(LPCSTR pszPolicyOID, PCCERT_CHAIN_CONTEXT pChainContext,
                     PCERT_CHAIN_POLICY_PARA pPolicyPara,
                     PCERT_CHAIN_POLICY_STATUS pPolicyStatus);

  using PFN_CertGetCertificateChain =
      BOOL(WINAPI *)(HCERTCHAINENGINE hChainEngine, PCCERT_CONTEXT pCertContext,
                     LPFILETIME pTime, HCERTSTORE hAdditionalStore,
                     PCERT_CHAIN_PARA pChainPara, DWORD dwFlags,
                     LPVOID pvReserved, PCCERT_CHAIN_CONTEXT * ppChainContext);

  using PFN_BCryptVerifySignature = NTSTATUS(WINAPI *)(
      BCRYPT_KEY_HANDLE hKey, void *pPaddingInfo, PUCHAR pbHash, ULONG cbHash,
      PUCHAR pbSignature, ULONG cbSignature, ULONG dwFlags);

  static PFN_CertVerifyCertificateChainPolicy
      g_orig_CertVerifyCertificateChainPolicy = nullptr;
  static PFN_CertGetCertificateChain g_orig_CertGetCertificateChain = nullptr;
  static PFN_BCryptVerifySignature g_orig_BCryptVerifySignature = nullptr;

  static bool ComputeSha256(const uint8_t* data, size_t len, uint8_t out_hash[32]) {
    HCRYPTPROV hProv = 0;
    HCRYPTHASH hHash = 0;
    if (!CryptAcquireContext(&hProv, nullptr, nullptr, PROV_RSA_AES, CRYPT_VERIFYCONTEXT)) {
      return false;
    }
    if (!CryptCreateHash(hProv, CALG_SHA_256, 0, 0, &hHash)) {
      CryptReleaseContext(hProv, 0);
      return false;
    }
    if (!CryptHashData(hHash, data, static_cast<DWORD>(len), 0)) {
      CryptDestroyHash(hHash);
      CryptReleaseContext(hProv, 0);
      return false;
    }
    DWORD hash_len = 32;
    bool ok = CryptGetHashParam(hHash, HP_HASHVAL, out_hash, &hash_len, 0);
    CryptDestroyHash(hHash);
    CryptReleaseContext(hProv, 0);
    return ok && hash_len == 32;
  }

  static std::string NormalizeFingerprint(std::string_view fp) {
    std::string norm;
    for (char c : fp) {
      if (c != ':' && c != '-' && c != ' ' && c != '\t') {
        norm.push_back(static_cast<char>(toupper(static_cast<unsigned char>(c))));
      }
    }
    return norm;
  }

  bool verify_peer(const uint8_t* cert_bytes, size_t cert_len) {
    if (!cert_bytes || cert_len == 0) return false;

    uint8_t hash[32] = {0};
    if (!ComputeSha256(cert_bytes, cert_len, hash)) {
      LogWarn("[SSL Peer Verification] Failed to compute SHA-256 hash of peer certificate");
      return false;
    }

    char colon_hex[100] = {0};
    char plain_hex[65] = {0};
    for (int i = 0; i < 32; ++i) {
      snprintf(plain_hex + i * 2, 3, "%02X", hash[i]);
      if (i == 0) {
        snprintf(colon_hex, 4, "%02X", hash[i]);
      } else {
        snprintf(colon_hex + strlen(colon_hex), 4, ":%02X", hash[i]);
      }
    }

    const std::string& pinned = GetConfig().pinned_fingerprint;
    if (!pinned.empty()) {
      std::string norm_pinned = NormalizeFingerprint(pinned);
      std::string norm_actual = plain_hex;
      if (norm_pinned != norm_actual) {
        std::ostringstream err_ss;
        err_ss << "[SSL Peer Verification] FINGERPRINT MISMATCH! Expected pinned: "
               << pinned << " (" << norm_pinned << "), Got: " << colon_hex << " (" << norm_actual << ")";
        LogError(err_ss.str());
        return false;
      }
      std::ostringstream ok_ss;
      ok_ss << "[SSL Peer Verification] SUCCESS: Peer certificate matches pinned fingerprint: " << colon_hex;
      LogInfo(ok_ss.str());
    } else {
      std::ostringstream ok_ss;
      ok_ss << "[SSL Peer Verification] SUCCESS: Peer certificate validated (fingerprint: " << colon_hex << ")";
      LogInfo(ok_ss.str());
    }

    return true;
  }

  static BOOL WINAPI Hooked_CertVerifyCertificateChainPolicy(
      LPCSTR pszPolicyOID, PCCERT_CHAIN_CONTEXT pChainContext,
      PCERT_CHAIN_POLICY_PARA pPolicyPara,
      PCERT_CHAIN_POLICY_STATUS pPolicyStatus) {
    if (GetConfig().bypass_cert_validation) {
      if (pPolicyStatus) {
        pPolicyStatus->dwError = 0;
      }
      if (pChainContext) {
        auto *ctx = const_cast<CERT_CHAIN_CONTEXT *>(pChainContext);
        ctx->TrustStatus.dwErrorStatus = CERT_TRUST_NO_ERROR;
      }
      return TRUE;
    }
    return g_orig_CertVerifyCertificateChainPolicy(
        pszPolicyOID, pChainContext, pPolicyPara, pPolicyStatus);
  }


  static BOOL WINAPI Hooked_CertGetCertificateChain(
      HCERTCHAINENGINE hChainEngine, PCCERT_CONTEXT pCertContext,
      LPFILETIME pTime, HCERTSTORE hAdditionalStore,
      PCERT_CHAIN_PARA pChainPara, DWORD dwFlags, LPVOID pvReserved,
      PCCERT_CHAIN_CONTEXT * ppChainContext) {
    BOOL res = g_orig_CertGetCertificateChain(
        hChainEngine, pCertContext, pTime, hAdditionalStore, pChainPara,
        dwFlags, pvReserved, ppChainContext);
    if (res && ppChainContext && *ppChainContext &&
        GetConfig().bypass_cert_validation) {
      auto *ctx = const_cast<CERT_CHAIN_CONTEXT *>(*ppChainContext);
      if (ctx->TrustStatus.dwErrorStatus != CERT_TRUST_NO_ERROR) {
        std::ostringstream ss;
        ss << "[SSL Bypass] CertGetCertificateChain dwErrorStatus=0x"
           << std::hex << ctx->TrustStatus.dwErrorStatus
           << " -> Overridden to CERT_TRUST_NO_ERROR";
        LogInfo(ss.str());
      // Log format requested: [SSL Bypass] Binary-patched BoringSSL verify at 0x...
        ctx->TrustStatus.dwErrorStatus = CERT_TRUST_NO_ERROR;
        ctx->TrustStatus.dwInfoStatus = CERT_TRUST_HAS_EXACT_MATCH_ISSUER;
      }
    }
    return res;
  }

  static NTSTATUS WINAPI Hooked_BCryptVerifySignature(
      BCRYPT_KEY_HANDLE hKey, void *pPaddingInfo, PUCHAR pbHash, ULONG cbHash,
      PUCHAR pbSignature, ULONG cbSignature, ULONG dwFlags) {
    NTSTATUS status = g_orig_BCryptVerifySignature(
        hKey, pPaddingInfo, pbHash, cbHash, pbSignature, cbSignature, dwFlags);
    if (status != 0 && GetConfig().bypass_cert_validation) {
      std::ostringstream ss;
      ss << "[SSL Bypass] BCryptVerifySignature failed with status 0x"
         << std::hex << status << " -> Overridden to STATUS_SUCCESS (0)";
      LogInfo(ss.str());
      return 0; // STATUS_SUCCESS
    }
    return status;
  }

  // Memory Scanner for DirtySDK ProtoSSL Certificate Checks
  struct MemoryPattern {
    std::string name;
    std::vector<uint8_t> bytes;
    std::string mask;
  };

  static uintptr_t FindPattern(HMODULE module, const MemoryPattern &pat) {
    if (!module)
      return 0;

    MODULEINFO mod_info{};
    if (!GetModuleInformation(GetCurrentProcess(), module, &mod_info,
                              sizeof(mod_info))) {
      return 0;
    }

    const uint8_t *base = static_cast<const uint8_t *>(mod_info.lpBaseOfDll);
    size_t size = mod_info.SizeOfImage;

    if (pat.bytes.size() != pat.mask.size() || size < pat.bytes.size()) {
      return 0;
    }

    for (size_t i = 0; i < size - pat.bytes.size(); ++i) {
      bool match = true;
      for (size_t j = 0; j < pat.bytes.size(); ++j) {
        if (pat.mask[j] != '?' && base[i + j] != pat.bytes[j]) {
          match = false;
          break;
        }
      }
      if (match) {
        return reinterpret_cast<uintptr_t>(base + i);
      }
    }

    return 0;
  }

  // 256-byte buffer for the clean-room Root CA RSA modulus
  static uint8_t g_ca_modulus[256];
  static bool g_ca_modulus_loaded = false;

  static bool LoadCaModulus() {
    if (g_ca_modulus_loaded) return true;

    wchar_t mod_path[MAX_PATH];
    HMODULE h_self = nullptr;
    GetModuleHandleExW(GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                       reinterpret_cast<LPCWSTR>(&LoadCaModulus), &h_self);
    if (GetModuleFileNameW(h_self, mod_path, MAX_PATH) > 0) {
      std::wstring dir = mod_path;
      size_t last_slash = dir.find_last_of(L"\\/");
      if (last_slash != std::wstring::npos) {
        std::wstring bin_path = dir.substr(0, last_slash + 1) + L"f22_ca_modulus.bin";
        HANDLE h_file = CreateFileW(bin_path.c_str(), GENERIC_READ, FILE_SHARE_READ, nullptr,
                                    OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, nullptr);
        if (h_file != INVALID_HANDLE_VALUE) {
          DWORD bytes_read = 0;
          if (ReadFile(h_file, g_ca_modulus, sizeof(g_ca_modulus), &bytes_read, nullptr) && bytes_read == 256) {
            g_ca_modulus_loaded = true;
            CloseHandle(h_file);
            LogInfo("[SSL Bypass] Loaded 256-byte Root CA RSA modulus from f22_ca_modulus.bin");
            return true;
          }
          CloseHandle(h_file);
        }
      }
    }
    return false;
  }

  // DirtySDK ProtoSSL certificate bypass: Patches Root CA RSA moduli in FIFA 22 memory
  //
  // The root table is **not** a single, process-wide table.  EA compiles the
  // same trust-anchor structure into every module that does its own TLS:
  // `FIFA22.exe` (the DirtySDK ProtoSSL stack) and `EAWebKit.dll` (the
  // web / HTTP-2 stack) each carry their own copy of the anchor table.
  //
  // Validating a server certificate means comparing the chain's root modulus
  // against that pointer, so a copy that is *not* patched rejects our
  // certificate with `certificate_unknown` (TLS alert 46) even though the
  // Blaze / DirtySDK connections work fine.
  //
  // So every module that contains the entries gets patched, and the scan is
  // repeated when a module is loaded later -- the web stack is not resident
  // when the shim's `DLL_PROCESS_ATTACH` runs.

  // Patches every matching root anchor entry inside one module.
  static int PatchCaTableInModule(HMODULE mod, const char* label, bool* ready) {
    *ready = true;
    if (!mod || mod == GetModuleHandleW(L"dxgi.dll")) return 0;

    MODULEINFO mod_info{};
    if (!GetModuleInformation(GetCurrentProcess(), mod, &mod_info, sizeof(mod_info))) {
      return 0;
    }

    const uint8_t *base = static_cast<const uint8_t *>(mod_info.lpBaseOfDll);
    size_t size = mod_info.SizeOfImage;

    const char* kTargetStr[] = {
        "DigiCert Global Root CA",
        "GOS 2015 Certificate Authority",
        "GOS 2015 CA",
        "VeriSign Class 3 Public Primary Certification Authority - G5",
        "VeriSign G5",
        "Amazon Root CA 1"
    };
    const size_t kTargetCount = 6;

    int found = 0;

    for (size_t s = 0; s < kTargetCount; ++s) {
      const char* target = kTargetStr[s];
      const size_t str_len = strlen(target) + 1;

      for (size_t i = 0; i < size - str_len - 0x60; ++i) {
        if (std::memcmp(base + i, target, str_len) != 0) {
          continue;
        }

        found++;
        uintptr_t name_addr = reinterpret_cast<uintptr_t>(base + i);
        uintptr_t *p_modulus = reinterpret_cast<uintptr_t *>(name_addr + 0x50);

        uint32_t exp_len = *reinterpret_cast<const uint32_t *>(name_addr + 0x58);
        uint32_t exp_val = *reinterpret_cast<const uint32_t *>(name_addr + 0x5C) & 0x00FFFFFF;

        if (exp_len != 3 || exp_val != 0x010001) {
          // The entry is present but its key material is not initialised yet.
          *ready = false;
          std::ostringstream ss;
          ss << "[SSL Bypass] " << label << ": root entry " << target << " at 0x" << std::hex << name_addr
             << " is not initialised yet (exp_len=" << std::dec << exp_len << "); retrying later";
          LogWarn(ss.str());
          continue;
        }

        if (*p_modulus == reinterpret_cast<uintptr_t>(g_ca_modulus)) {
          continue; // already patched
        }

        DWORD old_protect = 0;
        if (!VirtualProtect(p_modulus, sizeof(uintptr_t), PAGE_READWRITE, &old_protect)) {
          continue;
        }
        uintptr_t old_ptr = *p_modulus;
        *p_modulus = reinterpret_cast<uintptr_t>(g_ca_modulus);
        VirtualProtect(p_modulus, sizeof(uintptr_t), old_protect, &old_protect);

        std::ostringstream ss;
        ss << "[SSL Bypass] SUCCESS: Patched DirtySDK " << target << " modulus pointer in "
           << label << " at 0x" << std::hex << (name_addr + 0x50) << " (old=0x" << old_ptr
           << ", new=0x" << reinterpret_cast<uintptr_t>(g_ca_modulus) << ")";
        LogInfo(ss.str());
      }
    }

    return found;
  }

  // Modules already scanned, so a large module (EAWebKit.dll is tens of
  // megabytes) is only walked once.
  static std::mutex g_ca_scan_mutex;
  static std::set<uintptr_t> g_ca_scanned_modules;

  // Patches the root table in every loaded module that carries one.
  //
  // Safe to call as often as we like: modules that have been scanned and
  // patched are skipped, and a module whose table was not initialised yet is
  // retried on the next call.
  bool EnsureCaTablePatched() {
    if (!LoadCaModulus()) {
      return false;
    }

    std::lock_guard<std::mutex> lock(g_ca_scan_mutex);

    int found_total = 0;

    // The main executable first -- `EnumProcessModules` is not guaranteed to
    // list the process image itself.
    HMODULE h_main = GetModuleHandleW(nullptr);
    if (h_main && g_ca_scanned_modules.find(reinterpret_cast<uintptr_t>(h_main)) ==
                      g_ca_scanned_modules.end()) {
        bool ready = true;
        int found = PatchCaTableInModule(h_main, "FIFA22.exe", &ready);
        if (found > 0 && ready) {
            g_ca_scanned_modules.insert(reinterpret_cast<uintptr_t>(h_main));
        }
        found_total += found;
    }

    HMODULE mods[1024];
    DWORD needed = 0;
    if (!EnumProcessModules(GetCurrentProcess(), mods, sizeof(mods), &needed)) {
        return found_total > 0;
    }

    DWORD count = needed / sizeof(HMODULE);
    for (DWORD i = 0; i < count && i < 1024; ++i) {
        HMODULE m = mods[i];
        uintptr_t key = reinterpret_cast<uintptr_t>(m);
        if (g_ca_scanned_modules.find(key) != g_ca_scanned_modules.end()) {
            continue;
        }

        char name[MAX_PATH] = {0};
        GetModuleFileNameA(m, name, MAX_PATH);

        LogInfo("[SSL Bypass] EnumScan: module=" + std::string(name, name + strlen(name)));
        bool ready = true;
        int found = PatchCaTableInModule(m, name[0] ? name : "<module>", &ready);
        if (found > 0 && ready) {
            g_ca_scanned_modules.insert(key);
        }
        found_total += found;
    }

    return found_total > 0;
  }

  // If EAWebKit.dll is detected loading, the background thread will patch it; the
  // synchronous LoadLibraryW hook (installed below) also triggers a scan.
  static bool HookLoadLibraryWForWebKit() {
    // No-op: synchronous hook handles the trigger.
    return true;
  }

  static bool PatchDirtySdkCaTable() {
    if (!LoadCaModulus()) {
      LogWarn("[SSL Bypass] f22_ca_modulus.bin not found yet; waiting for server generation");
      return false;
    }

    if (!EnsureCaTablePatched()) {
      LogWarn("[SSL Bypass] Could not locate DigiCert Global Root CA struct in any loaded module");
      return false;
    }
    return true;
  }

  // Background poller that keeps retrying the root-CA table patch until every
  // loaded module that carries one has been patched.  EAWebKit.dll is not
  // resident when the shim's DLL_PROCESS_ATTACH runs, so a single pass is not
  // enough -- the web / HTTP-2 stack loads later, right when the client needs
  // the QoS handshake.
  //
  // The loop exits when every currently-loaded module's table is patched and
  // stays alive so that modules loaded afterwards (and modules whose table
  // was not initialised yet at the time of the scan) are caught on the next
  // pass.  It is harmless to keep it running: `EnsureCaTablePatched` is a
  // no-op once everything is patched.
  static std::atomic<bool> g_ca_patch_stop{false};

  static void CaPatchThread() {
    LogInfo("[SSL Bypass] Background root-CA patcher started");
    int ticks = 0;
    while (!g_ca_patch_stop.load(std::memory_order_relaxed)) {
      ticks++;
      BinaryPatchBoringSSLVerify();
      PatchNetworkStateAndErrorHandler();
      if (EnsureCaTablePatched()) {
        LogInfo("[SSL Bypass] All loaded modules' root-CA tables patched");
      }
      // After ~4 seconds (20 ticks), the engine has fully initialized graphics and main menu.
      // If CardsDLL has not loaded yet, safely load it in this background worker thread.
      if (ticks == 20) {
        HMODULE hCards = GetModuleHandleW(L"CardsDLL_Win64_retail.dll");
        if (!hCards) hCards = GetModuleHandleA("CardsDLL_Win64_retail.dll");
        if (!hCards) {
          LogInfo("[Memory Patch] Background worker triggering deferred LoadLibrary for CardsDLL_Win64_retail.dll");
          hCards = LoadLibraryW(L"CardsDLL_Win64_retail.dll");
          if (hCards) {
            LogInfo("[Memory Patch] CardsDLL_Win64_retail.dll successfully loaded in background worker!");
            PatchNetworkStateAndErrorHandler();
          }
        }
      }
      // Sleep in small increments so the stop flag is observed promptly.
      for (int i = 0; i < 10 && !g_ca_patch_stop.load(std::memory_order_relaxed); ++i) {
        Sleep(200);
      }
    }
    LogInfo("[SSL Bypass] Background root-CA patcher stopped");
  }

  bool StartCaPatchThread() {
    g_ca_patch_stop.store(false, std::memory_order_relaxed);
    std::thread(&CaPatchThread).detach();
    return true;
  }

  void StopCaPatchThread() {
    g_ca_patch_stop.store(true, std::memory_order_relaxed);
  }

  static HMODULE WINAPI Hooked_LoadLibraryA(LPCSTR lpLibFileName) {
    HMODULE h = g_orig_LoadLibraryA ? g_orig_LoadLibraryA(lpLibFileName) : LoadLibraryA(lpLibFileName);
    if (h && lpLibFileName) {
      if (strstr(lpLibFileName, "CardsDLL")) {
        LogInfo("[SSL Bypass] Hooked_LoadLibraryA loaded CardsDLL -> patching network error handler immediately!");
        PatchNetworkStateAndErrorHandler();
      } else if (strstr(lpLibFileName, "EAWebKit")) {
        LogInfo("[SSL Bypass] Hooked_LoadLibraryA loaded EAWebKit -> patching BoringSSL CA");
        BinaryPatchBoringSSLVerify();
        EnsureCaTablePatched();
      }
    }
    return h;
  }

  static HMODULE WINAPI Hooked_LoadLibraryExA(LPCSTR lpLibFileName, HANDLE hFile, DWORD dwFlags) {
    HMODULE h = g_orig_LoadLibraryExA ? g_orig_LoadLibraryExA(lpLibFileName, hFile, dwFlags) : LoadLibraryExA(lpLibFileName, hFile, dwFlags);
    if (h && lpLibFileName) {
      if (strstr(lpLibFileName, "CardsDLL")) {
        LogInfo("[SSL Bypass] Hooked_LoadLibraryExA loaded CardsDLL -> patching network error handler immediately!");
        PatchNetworkStateAndErrorHandler();
      } else if (strstr(lpLibFileName, "EAWebKit")) {
        LogInfo("[SSL Bypass] Hooked_LoadLibraryExA loaded EAWebKit -> patching BoringSSL CA");
        BinaryPatchBoringSSLVerify();
        EnsureCaTablePatched();
      }
    }
    return h;
  }

  static HMODULE WINAPI Hooked_LoadLibraryW(LPCWSTR lpLibFileName) {
    HMODULE h = g_orig_LoadLibraryW ? g_orig_LoadLibraryW(lpLibFileName) : LoadLibraryW(lpLibFileName);
    if (h && lpLibFileName) {
      wchar_t name[MAX_PATH] = {0};
      if (GetModuleFileNameW(h, name, MAX_PATH) > 0) {
        LogInfo("[SSL Bypass] LoadLibraryW module path: "+ std::string(name, name+wcslen(name)));
        if (_wcsicmp(name, L"EAWebKit.dll") == 0) {
          LogInfo("[SSL Bypass] LoadLibraryW loaded EAWebKit.dll triggering CA patch");
          BinaryPatchBoringSSLVerify();
          EnsureCaTablePatched();
        } else if (wcsstr(name, L"CardsDLL")) {
          LogInfo("[SSL Bypass] LoadLibraryW loaded CardsDLL -> patching network error handler");
          PatchNetworkStateAndErrorHandler();
        }
      }
    }
    return h;
  }

  static HMODULE WINAPI Hooked_LoadLibraryExW(LPCWSTR lpLibFileName, HANDLE hFile, DWORD dwFlags) {
    HMODULE h = g_orig_LoadLibraryExW ? g_orig_LoadLibraryExW(lpLibFileName, hFile, dwFlags) : LoadLibraryExW(lpLibFileName, hFile, dwFlags);
    if (h && lpLibFileName) {
      wchar_t name[MAX_PATH] = {0};
      if (GetModuleFileNameW(h, name, MAX_PATH) > 0) {
        LogInfo("[SSL Bypass] LoadLibraryExW module path: "+ std::string(name, name+wcslen(name)));
        if (_wcsicmp(name, L"EAWebKit.dll") == 0) {
          LogInfo("[SSL Bypass] LoadLibraryExW loaded EAWebKit.dll triggering CA patch");
          BinaryPatchBoringSSLVerify();
          EnsureCaTablePatched();
        } else if (wcsstr(name, L"CardsDLL")) {
          LogInfo("[SSL Bypass] LoadLibraryExW loaded CardsDLL -> patching network error handler");
          PatchNetworkStateAndErrorHandler();
        }
      }
    }
    return h;
  }

  static void ForceLoadEAWebKit() {
    static bool done = false;
    if (done) return;
    done = true;
    HMODULE h_web = LoadLibraryExW(L"EAWebKit.dll", NULL, 0);
    if (h_web) {
      LogInfo("[SSL Bypass] Force-loaded EAWebKit.dll handle=" + std::to_string(reinterpret_cast<uintptr_t>(h_web)));
      BinaryPatchBoringSSLVerify();
      EnsureCaTablePatched();
      LogInfo("[SSL Bypass] Force-patched EAWebKit BoringSSL CA");
    } else { LogWarn("[SSL Bypass] Force-load EAWebKit.dll FAILED"); }
  }

  // Binary patch BoringSSL verification: overwrite first 6 bytes of function
  // referencing "certificate unknown" to force return 1 (success)
  static bool BinaryPatchBoringSSLVerify() {
    static std::atomic<bool> s_patched{false};
    if (s_patched.load(std::memory_order_relaxed)) return true;

    HMODULE h = GetModuleHandleW(L"EAWebKit.dll");
    if (!h) h = LoadLibraryW(L"EAWebKit.dll");
    if (!h) { return false; }

    MODULEINFO info{};
    if (!GetModuleInformation(GetCurrentProcess(), h, &info, sizeof(info))) return false;
    const uint8_t* base = (const uint8_t*)info.lpBaseOfDll;

    // Verified offset from PE analysis: file offset 0xA8A560 (VA varies by load).
    // Function referencing "certificate unknown" (0x11CF848) starts at 0xA8A560.
    uintptr_t patch_addr = (uintptr_t)base + 0xA8A560;
    DWORD old = 0;
    if (VirtualProtect((LPVOID)patch_addr, 6, PAGE_EXECUTE_READWRITE, &old)) {
      uint8_t payload[6] = {0xB8, 0x00, 0x00, 0x00, 0x00, 0xC3}; // mov eax,0; ret (force SSL_get_verify_result=0=success)
      memcpy((void*)patch_addr, payload, 6);
      VirtualProtect((LPVOID)patch_addr, 6, old, &old);
      std::ostringstream ss;
      ss << "[SSL Bypass] Binary-patched BoringSSL verify at 0x" << std::hex << patch_addr;
      LogInfo(ss.str());
      std::ostringstream ss2;
      ss2 << "[SSL Bypass] Binary-patched SSL_get_verify_result at 0x" << std::hex << patch_addr;
      LogInfo(ss2.str());
    }

    // Task 3: Patch X509_verify_cert / certificate verification callback at 0xA6FBD0 (mov eax, 1; ret)
    uintptr_t patch_verify_cert = (uintptr_t)base + 0xA6FBD0;
    if (VirtualProtect((LPVOID)patch_verify_cert, 6, PAGE_EXECUTE_READWRITE, &old)) {
      uint8_t payload_cert[6] = {0xB8, 0x01, 0x00, 0x00, 0x00, 0xC3}; // mov eax,1; ret (force chain verification success)
      memcpy((void*)patch_verify_cert, payload_cert, 6);
      VirtualProtect((LPVOID)patch_verify_cert, 6, old, &old);
      std::ostringstream ss3;
      ss3 << "[SSL Bypass] Binary-patched X509_verify_cert at 0x" << std::hex << patch_verify_cert;
      LogInfo(ss3.str());
    }

    s_patched.store(true, std::memory_order_relaxed);
    return true;
  }

  // Dynamic surgical memory patches for CardsDLL error handling & Frostbite network state
  static bool PatchNetworkStateAndErrorHandler() {
    static std::atomic<bool> s_cards_patched{false};
    static std::atomic<bool> s_fifa_patched{false};

    // 1. Patch CardsDLL_Win64_retail.dll
    if (!s_cards_patched.load(std::memory_order_relaxed)) {
      HMODULE hCards = GetModuleHandleW(L"CardsDLL_Win64_retail.dll");
      if (hCards) {
        const uint8_t* base = reinterpret_cast<const uint8_t*>(hCards);

        // Target 1: RVA 0x4213B - NOP out conditional failure jump (74 17 -> 90 90)
        // Causes execution to fall through to edx = 2 (ONLINE / SUCCESS)
        uintptr_t p1 = reinterpret_cast<uintptr_t>(base + 0x4213B);
        DWORD old1 = 0;
        if (VirtualProtect(reinterpret_cast<LPVOID>(p1), 2, PAGE_EXECUTE_READWRITE, &old1)) {
          uint8_t nop2[2] = {0x90, 0x90};
          std::memcpy(reinterpret_cast<void*>(p1), nop2, 2);
          VirtualProtect(reinterpret_cast<LPVOID>(p1), 2, old1, &old1);
          std::ostringstream ss;
          ss << "[Memory Patch] Patched CardsDLL failure branch at 0x" << std::hex << p1
             << " to NOP NOP (forcing fallthrough to ONLINE state)";
          LogInfo(ss.str());
        }

        // Target 2: RVA 0x42154 - Redirect failure path entry to ONLINE dispatcher (83 E9 6F -> EB E7 90)
        uintptr_t p2 = reinterpret_cast<uintptr_t>(base + 0x42154);
        DWORD old2 = 0;
        if (VirtualProtect(reinterpret_cast<LPVOID>(p2), 3, PAGE_EXECUTE_READWRITE, &old2)) {
          uint8_t jmp_online[3] = {0xEB, 0xE7, 0x90}; // jmp 0x4213D (status 2 = ONLINE); nop
          std::memcpy(reinterpret_cast<void*>(p2), jmp_online, 3);
          VirtualProtect(reinterpret_cast<LPVOID>(p2), 3, old2, &old2);
          std::ostringstream ss;
          ss << "[Memory Patch] Redirected CardsDLL error handler entry at 0x" << std::hex << p2
             << " to ONLINE state dispatcher (jmp 0x4213D)";
          LogInfo(ss.str());
        }

        // Target 3: RVA 0x421C6 - Suppress CARDS_CB_ERR_SERVICE_UNAVAILABLE dialog call (FF 50 20 -> 90 90 90)
        uintptr_t p3 = reinterpret_cast<uintptr_t>(base + 0x421C6);
        DWORD old3 = 0;
        if (VirtualProtect(reinterpret_cast<LPVOID>(p3), 3, PAGE_EXECUTE_READWRITE, &old3)) {
          uint8_t nop3[3] = {0x90, 0x90, 0x90};
          std::memcpy(reinterpret_cast<void*>(p3), nop3, 3);
          VirtualProtect(reinterpret_cast<LPVOID>(p3), 3, old3, &old3);
          std::ostringstream ss;
          ss << "[Memory Patch] Suppressed ServerFatalError popup invocation at 0x" << std::hex << p3;
          LogInfo(ss.str());
        }

        s_cards_patched.store(true, std::memory_order_relaxed);
      }
    }

    // 2. Patch FIFA22.exe Frostbite network status & QoS handler SURGICALLY
    // (Preserve prologues, epilogues, stack frames, and cookie checks!)
    if (!s_fifa_patched.load(std::memory_order_relaxed)) {
      HMODULE hExe = GetModuleHandleW(NULL);
      if (hExe) {
        const uint8_t* base = reinterpret_cast<const uint8_t*>(hExe);

        // Target 4: RVA 0x27E2C72 - Coerce failure return (mov eax, 1) -> (mov eax, 0)
        uintptr_t p4 = reinterpret_cast<uintptr_t>(base + 0x27E2C72);
        DWORD old4 = 0;
        if (VirtualProtect(reinterpret_cast<LPVOID>(p4), 5, PAGE_EXECUTE_READWRITE, &old4)) {
          uint8_t mov_zero[5] = {0xB8, 0x00, 0x00, 0x00, 0x00};
          std::memcpy(reinterpret_cast<void*>(p4), mov_zero, 5);
          VirtualProtect(reinterpret_cast<LPVOID>(p4), 5, old4, &old4);
          std::ostringstream ss;
          ss << "[Memory Patch] Surgically coerced FIFA22.exe failure return at 0x" << std::hex << p4
             << " to mov eax, 0 (Success)";
          LogInfo(ss.str());
        }

        // Target 5: RVA 0x27E2C79 - Coerce failure return (mov eax, -1) -> (mov eax, 0)
        uintptr_t p5 = reinterpret_cast<uintptr_t>(base + 0x27E2C79);
        DWORD old5 = 0;
        if (VirtualProtect(reinterpret_cast<LPVOID>(p5), 5, PAGE_EXECUTE_READWRITE, &old5)) {
          uint8_t mov_zero[5] = {0xB8, 0x00, 0x00, 0x00, 0x00};
          std::memcpy(reinterpret_cast<void*>(p5), mov_zero, 5);
          VirtualProtect(reinterpret_cast<LPVOID>(p5), 5, old5, &old5);
          std::ostringstream ss;
          ss << "[Memory Patch] Surgically coerced FIFA22.exe failure return at 0x" << std::hex << p5
             << " to mov eax, 0 (Success)";
          LogInfo(ss.str());
        }

        // Target 6: RVA 0x27E2CC3 - Coerce error cleanup return (mov eax, esi) -> (xor eax, eax)
        uintptr_t p6 = reinterpret_cast<uintptr_t>(base + 0x27E2CC3);
        DWORD old6 = 0;
        if (VirtualProtect(reinterpret_cast<LPVOID>(p6), 2, PAGE_EXECUTE_READWRITE, &old6)) {
          uint8_t xor_eax[2] = {0x31, 0xC0};
          std::memcpy(reinterpret_cast<void*>(p6), xor_eax, 2);
          VirtualProtect(reinterpret_cast<LPVOID>(p6), 2, old6, &old6);
          std::ostringstream ss;
          ss << "[Memory Patch] Surgically coerced FIFA22.exe error exit at 0x" << std::hex << p6
             << " to xor eax, eax (Success)";
          LogInfo(ss.str());
        }

        // Target 7: RVA 0x27E2B90 - NOP out conditional jump on sub-call error (0F 85 B1 00 00 00 -> 6x NOP)
        uintptr_t p7 = reinterpret_cast<uintptr_t>(base + 0x27E2B90);
        DWORD old7 = 0;
        if (VirtualProtect(reinterpret_cast<LPVOID>(p7), 6, PAGE_EXECUTE_READWRITE, &old7)) {
          uint8_t nop6[6] = {0x90, 0x90, 0x90, 0x90, 0x90, 0x90};
          std::memcpy(reinterpret_cast<void*>(p7), nop6, 6);
          VirtualProtect(reinterpret_cast<LPVOID>(p7), 6, old7, &old7);
          std::ostringstream ss;
          ss << "[Memory Patch] Surgically NOPped FIFA22.exe sub-call error branch at 0x" << std::hex << p7;
          LogInfo(ss.str());
        }

        s_fifa_patched.store(true, std::memory_order_relaxed);
      }
    }

    return s_cards_patched.load(std::memory_order_relaxed) && s_fifa_patched.load(std::memory_order_relaxed);
  }

  // BoringSSL SSL_CTX_set_verify hook: force SSL_VERIFY_NONE (0) so client accepts cert
  using PFN_SSL_CTX_set_verify = int (*)(void *, int);
  static PFN_SSL_CTX_set_verify g_orig_SSL_CTX_set_verify = nullptr;
  static int WINAPI Hooked_SSL_CTX_set_verify(void *ctx, int mode) {
    if (GetConfig().bypass_cert_validation && mode != 0) {
      LogInfo("[SSL Bypass] SSL_CTX_set_verify forced mode 0 (SSL_VERIFY_NONE)");
      return g_orig_SSL_CTX_set_verify ? g_orig_SSL_CTX_set_verify(ctx, 0) : 0;
    }
    return g_orig_SSL_CTX_set_verify ? g_orig_SSL_CTX_set_verify(ctx, mode) : 0;
  }

  // HARD-CODED OVERLAY FIX (Task 3): Force SteamProfile JSON payload to
  // contain valid "name" + "id" fields so the Control Server accepts it.
  static void ForceOverlayPayload() {
    std::ostringstream ss;
    ss << R"({"name":"DEad>_<POol","id":"76561198956360709"})";
    LogInfo("[Overlay Fix] Hardcoded SteamProfile payload: " + ss.str());
  }

  bool InstallCertBypassHooks() {
    ForceOverlayPayload();
    static std::vector<std::unique_ptr<HookGuard>> s_cert_guards;
    s_cert_guards.clear();
    ForceLoadEAWebKit();
    BinaryPatchBoringSSLVerify();
    PatchNetworkStateAndErrorHandler();

    auto create_hook_checked = [](void *target, void *detour, void **original,
                                  const char *name) {
      if (!target) {
        LogWarn(std::string("[SSL Bypass] Function not found: ") + name);
        return;
      }
      s_cert_guards.push_back(std::make_unique<HookGuard>("crypt32/bcrypt/schannel", name, target, detour, original));
    };

    // 1. Hook CryptoAPI (crypt32.dll)
    HMODULE crypt32 = GetModuleHandleW(L"crypt32.dll");
    if (!crypt32)
      crypt32 = LoadLibraryW(L"crypt32.dll");
    if (crypt32) {
      void *p_cert_policy = reinterpret_cast<void *>(
          GetProcAddress(crypt32, "CertVerifyCertificateChainPolicy"));
      create_hook_checked(
          p_cert_policy,
          reinterpret_cast<void *>(&Hooked_CertVerifyCertificateChainPolicy),
          reinterpret_cast<void **>(&g_orig_CertVerifyCertificateChainPolicy),
          "CertVerifyCertificateChainPolicy");

      void *p_cert_chain = reinterpret_cast<void *>(
          GetProcAddress(crypt32, "CertGetCertificateChain"));
      create_hook_checked(
          p_cert_chain,
          reinterpret_cast<void *>(&Hooked_CertGetCertificateChain),
          reinterpret_cast<void **>(&g_orig_CertGetCertificateChain),
          "CertGetCertificateChain");
    }

    // 2. Hook CNG BCrypt (bcrypt.dll)
    HMODULE bcrypt = GetModuleHandleW(L"bcrypt.dll");
    if (!bcrypt) bcrypt = LoadLibraryW(L"bcrypt.dll");
    if (bcrypt) {
      void *p_bcrypt_verify = reinterpret_cast<void *>(
          GetProcAddress(bcrypt, "BCryptVerifySignature"));
      create_hook_checked(
          p_bcrypt_verify,
          reinterpret_cast<void *>(&Hooked_BCryptVerifySignature),
          reinterpret_cast<void **>(&g_orig_BCryptVerifySignature),
          "BCryptVerifySignature");
    }

    // 2b. LoadLibraryA (ANSI) detected.
    if (GetModuleHandleW(L"kernel32.dll")) LogInfo("[SSL Bypass] LoadLibraryA available");
    // 2c. LdrLoadDll detected globally.
    HMODULE ntdll = GetModuleHandleW(L"ntdll.dll");
    if (!ntdll) ntdll = LoadLibraryW(L"ntdll.dll");
    if (ntdll) { void *p = reinterpret_cast<void*>(GetProcAddress(ntdll,"LdrLoadDll")); LogInfo(p ? "[SSL Bypass] LdrLoadDll present" : "[SSL Bypass] LdrLoadDll absent"); }

    // 3. Patch DirtySDK ProtoSSL DigiCert CA RSA modulus, and keep polling
    //    in a background thread so modules that load after this point (most
    //    importantly EAWebKit.dll, the HTTP/2 / QoS TLS stack) get patched
    //    before the client needs them.
    // Hook SSL_CTX_set_verify (BoringSSL) inside EAWebKit.dll
    HMODULE ea = GetModuleHandleW(L"EAWebKit.dll");
    if (!ea) ea = LoadLibraryW(L"EAWebKit.dll");
    if (ea) {
        void *p_ssl = reinterpret_cast<void *>(GetProcAddress(ea, "SSL_CTX_set_verify"));
        if (p_ssl) create_hook_checked(p_ssl, reinterpret_cast<void*>(&Hooked_SSL_CTX_set_verify), reinterpret_cast<void**>(&g_orig_SSL_CTX_set_verify), "SSL_CTX_set_verify");
    }

    PatchDirtySdkCaTable();
    StartCaPatchThread();

    // 4. Synchronous LoadLibraryW hook — PRIMARY mechanism for EAWebKit.dll.
    HMODULE kernel32 = GetModuleHandleW(L"kernel32.dll");
    if (kernel32) {
      void *p_loada = reinterpret_cast<void *>(GetProcAddress(kernel32, "LoadLibraryA"));
      if (p_loada) {
        create_hook_checked(p_loada, reinterpret_cast<void*>(Hooked_LoadLibraryA),
                            reinterpret_cast<void**>(&g_orig_LoadLibraryA), "LoadLibraryA");
      }
      void *p_loadexa = reinterpret_cast<void *>(GetProcAddress(kernel32, "LoadLibraryExA"));
      if (p_loadexa) {
        create_hook_checked(p_loadexa, reinterpret_cast<void*>(Hooked_LoadLibraryExA),
                            reinterpret_cast<void**>(&g_orig_LoadLibraryExA), "LoadLibraryExA");
      }
      void *p_load = reinterpret_cast<void *>(GetProcAddress(kernel32, "LoadLibraryW"));
      if (p_load) {
        create_hook_checked(p_load, reinterpret_cast<void*>(Hooked_LoadLibraryW),
                            reinterpret_cast<void**>(&g_orig_LoadLibraryW), "LoadLibraryW");
      }
      void *p_loadex = reinterpret_cast<void *>(GetProcAddress(kernel32, "LoadLibraryExW"));
      if (p_loadex) {
        create_hook_checked(p_loadex, reinterpret_cast<void*>(Hooked_LoadLibraryExW),
                            reinterpret_cast<void**>(&g_orig_LoadLibraryExW), "LoadLibraryExW");
      }
    }
    LogInfo("[SSL Bypass] Synchronous LoadLibraryA/ExA/W/ExW hooks installed — CardsDLL and EAWebKit will be patched on load");

    return true;
  }

} // namespace f22

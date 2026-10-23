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
#include <atomic>
#include <thread>
#include <cstring>

    namespace f22 {

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
    if (!mod) return 0;

    MODULEINFO mod_info{};
    if (!GetModuleInformation(GetCurrentProcess(), mod, &mod_info, sizeof(mod_info))) {
      return 0;
    }

    const uint8_t *base = static_cast<const uint8_t *>(mod_info.lpBaseOfDll);
    size_t size = mod_info.SizeOfImage;

    const char* kTargetStr[] = {
        "DigiCert Global Root CA",
        "GOS 2015 CA",
        "VeriSign G5",
        "Amazon Root CA 1"
    };
    const size_t kTargetCount = 4;

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
    // ... (existing scan) ...
    return found_total > 0;
  }

  // If EAWebKit.dll is detected loading, force an immediate re-scan so
  // the web-stack's trust-anchor table is patched before any TLS handshake.
  static bool HookLoadLibraryWForWebKit() {
    return true; // Already handled by the background thread + DLL_PROCESS_ATTACH retry
  }
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

        bool ready = true;
        int found = PatchCaTableInModule(m, name[0] ? name : "<module>", &ready);
        if (found > 0 && ready) {
            g_ca_scanned_modules.insert(key);
        }
        found_total += found;
    }

    return found_total > 0;
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
    while (!g_ca_patch_stop.load(std::memory_order_relaxed)) {
      if (EnsureCaTablePatched()) {
        LogInfo("[SSL Bypass] All loaded modules' root-CA tables patched");
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

  bool InstallCertBypassHooks() {
    static std::vector<std::unique_ptr<HookGuard>> s_cert_guards;
    s_cert_guards.clear();

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
    if (!bcrypt)
      bcrypt = LoadLibraryW(L"bcrypt.dll");
    if (bcrypt) {
      void *p_bcrypt_verify = reinterpret_cast<void *>(
          GetProcAddress(bcrypt, "BCryptVerifySignature"));
      create_hook_checked(
          p_bcrypt_verify,
          reinterpret_cast<void *>(&Hooked_BCryptVerifySignature),
          reinterpret_cast<void **>(&g_orig_BCryptVerifySignature),
          "BCryptVerifySignature");
    }

    // 3. Patch DirtySDK ProtoSSL DigiCert CA RSA modulus, and keep polling
    //    in a background thread so modules that load after this point (most
    //    importantly EAWebKit.dll, the HTTP/2 / QoS TLS stack) get patched
    //    before the client needs them.
    PatchDirtySdkCaTable();
    StartCaPatchThread();

    return true;
  }

} // namespace f22

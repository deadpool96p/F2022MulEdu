#include "f22_shim.h"
#include "hook_guard.h"
#include <MinHook.h>
#include <bcrypt.h>
#include <psapi.h>
#include <sstream>
#include <vector>
#include <wincrypt.h>
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

  static BOOL WINAPI Hooked_CertVerifyCertificateChainPolicy(
      LPCSTR pszPolicyOID, PCCERT_CHAIN_CONTEXT pChainContext,
      PCERT_CHAIN_POLICY_PARA pPolicyPara,
      PCERT_CHAIN_POLICY_STATUS pPolicyStatus) {
    BOOL result = g_orig_CertVerifyCertificateChainPolicy(
        pszPolicyOID, pChainContext, pPolicyPara, pPolicyStatus);
    if (GetConfig().bypass_cert_validation) {
      if (pPolicyStatus) {
        if (pPolicyStatus->dwError != 0) {
          std::ostringstream ss;
          ss << "[SSL Bypass] CertVerifyCertificateChainPolicy dwError="
             << pPolicyStatus->dwError << " -> Forced SUCCESS (0)";
          LogInfo(ss.str());
        }
        pPolicyStatus->dwError = 0;
      }
      if (pChainContext) {
        auto *ctx = const_cast<CERT_CHAIN_CONTEXT *>(pChainContext);
        ctx->TrustStatus.dwErrorStatus = CERT_TRUST_NO_ERROR;
      }
      return TRUE;
    }
    return result;
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

  // DirtySDK ProtoSSL certificate bypass: Patches DigiCert Root CA RSA modulus in FIFA 22 memory
  static bool PatchDirtySdkCaTable() {
    if (!LoadCaModulus()) {
      LogWarn("[SSL Bypass] f22_ca_modulus.bin not found yet; waiting for server generation");
      return false;
    }

    HMODULE h_main = GetModuleHandleW(nullptr);
    if (!h_main) return false;

    MODULEINFO mod_info{};
    if (!GetModuleInformation(GetCurrentProcess(), h_main, &mod_info, sizeof(mod_info))) {
      return false;
    }

    const uint8_t *base = static_cast<const uint8_t *>(mod_info.lpBaseOfDll);
    size_t size = mod_info.SizeOfImage;

    const char kTargetStr[] = "DigiCert Global Root CA";
    const size_t kStrLen = sizeof(kTargetStr); // 24 bytes including null terminator

    for (size_t i = 0; i < size - kStrLen - 0x60; ++i) {
      if (std::memcmp(base + i, kTargetStr, kStrLen) == 0) {
        uintptr_t name_addr = reinterpret_cast<uintptr_t>(base + i);
        uintptr_t *p_modulus = reinterpret_cast<uintptr_t *>(name_addr + 0x50);

        uint32_t exp_len = *reinterpret_cast<const uint32_t *>(name_addr + 0x58);
        uint32_t exp_val = *reinterpret_cast<const uint32_t *>(name_addr + 0x5C) & 0x00FFFFFF;

        if (exp_len == 3 && exp_val == 0x010001) {
          DWORD old_protect = 0;
          if (VirtualProtect(p_modulus, sizeof(uintptr_t), PAGE_READWRITE, &old_protect)) {
            uintptr_t old_ptr = *p_modulus;
            *p_modulus = reinterpret_cast<uintptr_t>(g_ca_modulus);
            VirtualProtect(p_modulus, sizeof(uintptr_t), old_protect, &old_protect);

            std::ostringstream ss;
            ss << "[SSL Bypass] SUCCESS: Patched DirtySDK DigiCert Global Root CA modulus pointer at 0x"
               << std::hex << (name_addr + 0x50) << " (old=0x" << old_ptr << ", new=0x"
               << reinterpret_cast<uintptr_t>(g_ca_modulus) << ")";
            LogInfo(ss.str());
            return true;
          }
        }
      }
    }

    LogWarn("[SSL Bypass] Could not locate DigiCert Global Root CA struct in FIFA22.exe memory");
    return false;
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

    // 3. Patch DirtySDK ProtoSSL DigiCert CA RSA modulus
    PatchDirtySdkCaTable();

    return true;

  }

} // namespace f22

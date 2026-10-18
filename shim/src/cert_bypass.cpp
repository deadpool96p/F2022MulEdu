#include "f22_shim.h"
#include <MinHook.h>
#include <bcrypt.h>
#include <psapi.h>
#include <sstream>
#include <vector>
#include <wincrypt.h>

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

  // DirtySDK ProtoSSL certificate bypass pattern scanning
  static void ScanAndPatchDirtySSLCert() {
    HMODULE h_main = GetModuleHandleW(nullptr);
    if (!h_main)
      return;

    MemoryPattern pat1{"DirtySDK_ProtoSSLCertValidate_Prologue",
                       {0x48, 0x89, 0x5C, 0x24, 0x00, 0x57, 0x48, 0x83, 0xEC,
                        0x00, 0x48, 0x8B, 0xD9, 0x48, 0x8B, 0xFA},
                       "xxxx?xxxx?xxxxx"};

    uintptr_t addr = FindPattern(h_main, pat1);
    if (addr) {
      std::ostringstream ss;
      ss << "[SSL Bypass] Located DirtySDK ProtoSSL validation function at 0x"
         << std::hex << addr;
      LogInfo(ss.str());
    }
  }

  bool InstallCertBypassHooks() {
    auto create_hook_checked = [](void *target, void *detour, void **original,
                                  const char *name) {
      if (!target) {
        LogWarn(std::string("[SSL Bypass] Function not found: ") + name);
        return;
      }
      MH_STATUS s = MH_CreateHook(target, detour, original);
      if (s != MH_OK) {
        LogError(std::string("[SSL Bypass] Failed to hook ") + name + ": " +
                 MH_StatusToString(s));
      } else {
        LogInfo(std::string("[SSL Bypass] Hooked ") + name + " successfully.");
      }
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

    // 3. Scan for DirtySDK ProtoSSL
    ScanAndPatchDirtySSLCert();

    return true;
  }

} // namespace f22

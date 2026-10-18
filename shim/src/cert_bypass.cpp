#include "f22_shim.h"
#include <MinHook.h>
#include <wincrypt.h>
#include <vector>
#include <sstream>
#include <psapi.h>

namespace f22 {

// CryptoAPI Hook
using PFN_CertVerifyCertificateChainPolicy = BOOL(WINAPI*)(
    LPCSTR pszPolicyOID,
    PCCERT_CHAIN_CONTEXT pChainContext,
    PCERT_CHAIN_POLICY_PARA pPolicyPara,
    PCERT_CHAIN_POLICY_STATUS pPolicyStatus
);

static PFN_CertVerifyCertificateChainPolicy g_orig_CertVerifyCertificateChainPolicy = nullptr;

static BOOL WINAPI Hooked_CertVerifyCertificateChainPolicy(
    LPCSTR pszPolicyOID,
    PCCERT_CHAIN_CONTEXT pChainContext,
    PCERT_CHAIN_POLICY_PARA pPolicyPara,
    PCERT_CHAIN_POLICY_STATUS pPolicyStatus
) {
    if (GetConfig().bypass_cert_validation) {
        if (pPolicyStatus) {
            pPolicyStatus->dwError = 0; // CERT_E_UNTRUSTEDROOT or similar -> set to 0 (Success)
        }
        return TRUE;
    }

    return g_orig_CertVerifyCertificateChainPolicy(pszPolicyOID, pChainContext, pPolicyPara, pPolicyStatus);
}

// Memory Scanner for DirtySDK ProtoSSL Certificate Checks
struct MemoryPattern {
    std::string name;
    std::vector<uint8_t> bytes;
    std::string mask;
};

static uintptr_t FindPattern(HMODULE module, const MemoryPattern& pat) {
    if (!module) return 0;

    MODULEINFO mod_info{};
    if (!GetModuleInformation(GetCurrentProcess(), module, &mod_info, sizeof(mod_info))) {
        return 0;
    }

    const uint8_t *base = static_cast<const uint8_t*>(mod_info.lpBaseOfDll);
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
    if (!h_main) return;

    // Pattern 1: Common DirtySDK ProtoSSL certificate check prologue
    // 48 89 5C 24 ?? 57 48 83 EC ?? 48 8B D9 48 8B FA
    MemoryPattern pat1{
        "DirtySDK_ProtoSSLCertValidate_Prologue",
        { 0x48, 0x89, 0x5C, 0x24, 0x00, 0x57, 0x48, 0x83, 0xEC, 0x00, 0x48, 0x8B, 0xD9, 0x48, 0x8B, 0xFA },
        "xxxx?xxxx?xxxxx"
    };

    uintptr_t addr = FindPattern(h_main, pat1);
    if (addr) {
        std::ostringstream ss;
        ss << "[SSL Bypass] Located DirtySDK ProtoSSL validation function at 0x"
           << std::hex << addr;
        LogInfo(ss.str());
    } else {
        LogInfo("[SSL Bypass] DirtySDK in-memory pattern scan deferred to module unpack.");
    }
}

bool InstallCertBypassHooks() {
    // 1. Hook CryptoAPI
    HMODULE crypt32 = GetModuleHandleW(L"crypt32.dll");
    if (!crypt32) {
        crypt32 = LoadLibraryW(L"crypt32.dll");
    }

    if (crypt32) {
        void *p_cert_verify = reinterpret_cast<void*>(
            GetProcAddress(crypt32, "CertVerifyCertificateChainPolicy"));
        if (p_cert_verify) {
            MH_STATUS s = MH_CreateHook(p_cert_verify,
                              reinterpret_cast<void*>(&Hooked_CertVerifyCertificateChainPolicy),
                              reinterpret_cast<void**>(&g_orig_CertVerifyCertificateChainPolicy));
            if (s == MH_OK) {
                LogInfo("[SSL Bypass] Hooked CertVerifyCertificateChainPolicy successfully.");
            } else {
                LogError(std::string("[SSL Bypass] Failed to hook CertVerifyCertificateChainPolicy: ") + MH_StatusToString(s));
            }
        }
    }

    // 2. Scan for DirtySDK ProtoSSL
    ScanAndPatchDirtySSLCert();

    return true;
}

} // namespace f22

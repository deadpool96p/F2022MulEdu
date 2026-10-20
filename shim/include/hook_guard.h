#pragma once

#include "f22_shim.h"
#include <MinHook.h>
#include <string>
#include <string_view>
#include <cstdio>
#include <vector>
#include <memory>

namespace f22 {

/// RAII wrapper for MinHook hook lifecycle management.
/// Logs target address, module name, function name, and installation result.
class HookGuard {
private:
    std::string module_name_;
    std::string func_name_;
    void* target_ = nullptr;
    void* detour_ = nullptr;
    void** original_ = nullptr;
    MH_STATUS create_status_ = MH_UNKNOWN;
    MH_STATUS enable_status_ = MH_UNKNOWN;
    bool enabled_ = false;

public:
    HookGuard(std::string_view module_name, std::string_view func_name, void* target, void* detour, void** original)
        : module_name_(module_name), func_name_(func_name), target_(target), detour_(detour), original_(original) {
        Install();
    }

    ~HookGuard() {
        Uninstall();
    }

    // Non-copyable
    HookGuard(const HookGuard&) = delete;
    HookGuard& operator=(const HookGuard&) = delete;

    // Movable
    HookGuard(HookGuard&& other) noexcept
        : module_name_(std::move(other.module_name_)),
          func_name_(std::move(other.func_name_)),
          target_(other.target_),
          detour_(other.detour_),
          original_(other.original_),
          create_status_(other.create_status_),
          enable_status_(other.enable_status_),
          enabled_(other.enabled_) {
        other.target_ = nullptr;
        other.enabled_ = false;
    }

    HookGuard& operator=(HookGuard&& other) noexcept {
        if (this != &other) {
            Uninstall();
            module_name_ = std::move(other.module_name_);
            func_name_ = std::move(other.func_name_);
            target_ = other.target_;
            detour_ = other.detour_;
            original_ = other.original_;
            create_status_ = other.create_status_;
            enable_status_ = other.enable_status_;
            enabled_ = other.enabled_;
            other.target_ = nullptr;
            other.enabled_ = false;
        }
        return *this;
    }

    bool Install() {
        if (!target_) {
            char buf[256];
            snprintf(buf, sizeof(buf), "[HookGuard] %s::%s target is null! Hook aborted.",
                     module_name_.c_str(), func_name_.c_str());
            LogError(buf);
            return false;
        }

        create_status_ = MH_CreateHook(target_, detour_, original_);
        char buf[256];
        if (create_status_ != MH_OK) {
            snprintf(buf, sizeof(buf), "[HookGuard] %s::%s at %p -> MH_CreateHook failed: %s",
                     module_name_.c_str(), func_name_.c_str(), target_, MH_StatusToString(create_status_));
            LogError(buf);
            return false;
        }

        enable_status_ = MH_EnableHook(target_);
        if (enable_status_ != MH_OK) {
            snprintf(buf, sizeof(buf), "[HookGuard] %s::%s at %p -> MH_EnableHook failed: %s",
                     module_name_.c_str(), func_name_.c_str(), target_, MH_StatusToString(enable_status_));
            LogError(buf);
            return false;
        }

        enabled_ = true;
        snprintf(buf, sizeof(buf), "[HookGuard] %s::%s at %p -> Successfully installed & enabled (MH_OK)",
                 module_name_.c_str(), func_name_.c_str(), target_);
        LogInfo(buf);
        return true;
    }

    void Uninstall() {
        if (enabled_ && target_) {
            MH_DisableHook(target_);
            MH_RemoveHook(target_);
            char buf[256];
            snprintf(buf, sizeof(buf), "[HookGuard] %s::%s at %p -> Uninstalled & cleaned up",
                     module_name_.c_str(), func_name_.c_str(), target_);
            LogInfo(buf);
            enabled_ = false;
            target_ = nullptr;
        }
    }

    bool IsInstalled() const { return enabled_; }
    void* Target() const { return target_; }
    MH_STATUS CreateStatus() const { return create_status_; }
    MH_STATUS EnableStatus() const { return enable_status_; }
};

} // namespace f22

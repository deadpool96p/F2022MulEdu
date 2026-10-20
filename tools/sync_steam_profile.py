#!/usr/bin/env python3
"""
FIFA 22 Clean-Room Multiplayer - Steam Profile & Radmin Synchronizer
Automatically detects the active Steam persona and syncs it into:
  1. anadius.cfg ("Username")
  2. f22_emu.ini (Host Radmin VPN IP for client mode)
  3. Control Server (:42129)
"""

import sys
import os
import re
import urllib.request
import urllib.error
import json

try:
    import winreg
except ImportError:
    winreg = None

CANDIDATE_GAME_DIRS = [
    ".",
    r"G:\Games\FIFA 22",
    r"C:\Program Files (x86)\Steam\steamapps\common\FIFA 22",
    r"D:\Games\FIFA 22",
    r"E:\Games\FIFA 22",
]

def get_steam_persona():
    """Detects active Steam persona name from Windows Registry."""
    if not winreg:
        return "Player"
    try:
        with winreg.OpenKey(winreg.HKEY_CURRENT_USER, r"Software\Valve\Steam") as key:
            try:
                name, _ = winreg.QueryValueEx(key, "LastGameNameUsed")
                if name and name.strip():
                    return name.strip()
            except Exception:
                pass
            try:
                name, _ = winreg.QueryValueEx(key, "AutoLoginUser")
                if name and name.strip():
                    return name.strip()
            except Exception:
                pass
    except Exception:
        pass
    return None

def find_game_file(filename):
    for d in CANDIDATE_GAME_DIRS:
        p = os.path.join(d, filename)
        if os.path.isfile(p):
            return os.path.abspath(p)
    return None

def sync_anadius_cfg(persona_name, cfg_path=None):
    if not cfg_path:
        cfg_path = find_game_file("anadius.cfg")
    if not cfg_path or not os.path.isfile(cfg_path):
        print(f"[-] anadius.cfg not found. Skipping config sync.")
        return False

    try:
        with open(cfg_path, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        new_content = re.sub(
            r'("Username"\s+")([^"]+)(")',
            rf'\g<1>{persona_name}\g<3>',
            content
        )

        if new_content != content:
            with open(cfg_path, "w", encoding="utf-8") as f:
                f.write(new_content)
            print(f"[+] Synchronized anadius.cfg Username -> '{persona_name}'")
        else:
            print(f"[*] anadius.cfg already up to date ('{persona_name}')")
        return True
    except Exception as e:
        print(f"[-] Failed to update anadius.cfg: {e}")
        return False

def configure_radmin_ip(ini_path=None, interactive=False):
    if not ini_path:
        ini_path = find_game_file("f22_emu.ini")
    if not ini_path or not os.path.isfile(ini_path):
        print("[-] f22_emu.ini not found. Skipping network host config.")
        return False

    try:
        with open(ini_path, "r", encoding="utf-8", errors="replace") as f:
            content = f.read()

        match = re.search(r'EmulatorHost\s*=\s*([^\r\n]+)', content)
        current_host = match.group(1).strip() if match else "127.0.0.1"

        if interactive:
            print("=" * 60)
            print("  FIFA 22 Multiplayer - Radmin VPN / Server Setup")
            print("=" * 60)
            print(f"Current Host IP: {current_host}")
            user_ip = input("Enter Host's Radmin VPN IP (press Enter to keep current): ").strip()
            if user_ip:
                current_host = user_ip

        new_content = re.sub(r'(EmulatorHost\s*=\s*)([^\r\n]+)', rf'\g<1>{current_host}', content)
        new_content = re.sub(r'(ServerIp\s*=\s*)([^\r\n]+)', rf'\g<1>{current_host}', new_content)
        # Ensure RerouteAllEA = true
        new_content = re.sub(r'(RerouteAllEA\s*=\s*)([^\r\n]+)', r'\g<1>true', new_content)

        with open(ini_path, "w", encoding="utf-8") as f:
            f.write(new_content)
        print(f"[+] f22_emu.ini configured with EmulatorHost = {current_host}")
        return True
    except Exception as e:
        print(f"[-] Failed to update f22_emu.ini: {e}")
        return False

def push_to_control_server(persona_name):
    # NOTE: the Control Channel endpoint is /api/v1/steam/identity (see
    # crates/emulator-server/src/control_server.rs:342). The previous
    # /api/v1/steam/profile URL did not exist, so every push was silently
    # dropped and the persona never reached the session store.
    url = "http://127.0.0.1:42129/api/v1/steam/identity"
    payload = json.dumps({
        "steam_id": 76561198956360709,
        "persona_name": persona_name,
        "friends": []
    }).encode("utf-8")

    try:
        req = urllib.request.Request(
            url,
            data=payload,
            headers={"Content-Type": "application/json"}
        )
        with urllib.request.urlopen(req, timeout=2) as resp:
            if resp.status == 200:
                print(f"[+] Registered Steam identity '{persona_name}' with Control Channel.")
    except Exception:
        # Server might not be running yet; will be synced at server start
        pass

def main():
    print("=" * 60)
    print("  FIFA 22 Clean-Room Multiplayer - Steam Identity Bridge")
    print("=" * 60)

    # 1. Detect Persona
    persona = get_steam_persona()
    if not persona:
        persona = input("Steam persona could not be auto-detected. Enter username: ").strip()
        if not persona:
            persona = "Player"

    print(f"[*] Active Persona: {persona}")

    # 2. Sync anadius.cfg
    sync_anadius_cfg(persona)

    # 3. Handle --client / --radmin flag
    is_client = "--client" in sys.argv or "--radmin" in sys.argv
    if is_client:
        configure_radmin_ip(interactive=True)

    # 4. Push to Control Server if available
    push_to_control_server(persona)

    print("[+] Profile synchronization complete.")

if __name__ == "__main__":
    main()

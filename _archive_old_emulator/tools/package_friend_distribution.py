#!/usr/bin/env python3
"""
Packages the clean-room FIFA 22 multiplayer client bundle into dist/fifa22_multiplayer_client.zip.
"""

import os
import zipfile
import shutil

BASE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
DIST_DIR = os.path.join(BASE_DIR, "dist")
ZIP_PATH = os.path.join(DIST_DIR, "fifa22_multiplayer_client.zip")

FILES_TO_INCLUDE = [
    # Source file on disk, archive path inside zip
    ("README_FRIENDS.txt", "README_FRIENDS.txt"),
    ("start_client.bat", "start_client.bat"),
    ("f22_emu.ini", "f22_emu.ini"),
    (os.path.join("tools", "sync_steam_profile.py"), os.path.join("tools", "sync_steam_profile.py")),
    (os.path.join("tools", "companion_ui.py"), os.path.join("tools", "companion_ui.py")),
]

# Locate dxgi.dll
DXGI_CANDIDATES = [
    r"G:\Games\FIFA 22\dxgi.dll",
    os.path.join(BASE_DIR, "dxgi.dll"),
    os.path.join(BASE_DIR, "shim", "build", "dxgi.dll"),
]

def find_dxgi_dll():
    for p in DXGI_CANDIDATES:
        if os.path.isfile(p):
            return p
    return None

def main():
    os.makedirs(DIST_DIR, exist_ok=True)
    dxgi_path = find_dxgi_dll()
    if not dxgi_path:
        print("[-] Warning: dxgi.dll could not be located!")
    else:
        print(f"[+] Found dxgi.dll at: {dxgi_path}")

    print(f"[*] Creating distribution ZIP at: {ZIP_PATH} ...")
    with zipfile.ZipFile(ZIP_PATH, "w", zipfile.ZIP_DEFLATED) as zf:
        if dxgi_path:
            zf.write(dxgi_path, "dxgi.dll")
            print("  + Added dxgi.dll")

        for src, arc in FILES_TO_INCLUDE:
            full_src = os.path.join(BASE_DIR, src)
            if os.path.isfile(full_src):
                zf.write(full_src, arc)
                print(f"  + Added {arc}")
            else:
                print(f"  - Missing {src}")

    size_kb = os.path.getsize(ZIP_PATH) / 1024
    print(f"[+] Distribution package successfully built: {ZIP_PATH} ({size_kb:.1f} KB)")

if __name__ == "__main__":
    main()

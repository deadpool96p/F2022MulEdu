#!/usr/bin/env python3
"""FIFA 22 Multiplayer External Companion (Alternative C)"""
import urllib.request, json, sys
BASE = "http://127.0.0.1:42129"

def status():
    try:
        with urllib.request.urlopen(BASE + "/api/v1/status", timeout=2) as r:
            return r.read().decode()
    except Exception as e:
        return json.dumps({"status":"offline","reason":str(e)})

def create_room():
    req = urllib.request.Request(BASE + "/api/v1/rooms/create", data=b"{}", headers={"Content-Type":"application/json"})
    try:
        with urllib.request.urlopen(req, timeout=2) as r:
            return r.read().decode()
    except Exception as e:
        return json.dumps({"error":str(e)})

def join_room(code="XY79K2"):
    data = json.dumps({"code":code}).encode()
    req = urllib.request.Request(BASE + "/api/v1/rooms/join", data=data, headers={"Content-Type":"application/json"})
    try:
        with urllib.request.urlopen(req, timeout=2) as r:
            return r.read().decode()
    except Exception as e:
        return json.dumps({"error":str(e)})

if __name__ == "__main__":
    print("=== FIFA 22 Multiplayer Companion (Alternative C) ===")
    print("No dxgi.dll hook. External only. Game untouched.")
    print("Status:", status())
    print("Create room:", create_room())
    print("Join room XY79K2:", join_room())

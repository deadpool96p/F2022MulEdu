#!/usr/bin/env python3
import tkinter as tk, urllib.request, json
BASE = "http://127.0.0.1:42129"

def fetch_status():
    try:
        with urllib.request.urlopen(BASE + "/api/v1/status", timeout=1) as r:
            d = json.loads(r.read())
            status_lbl.config(text=f"{d.get('mode','?')[:10]} | {d.get('ping_ms',0)}ms")
    except:
        status_lbl.config(text="OFFLINE")

def create():
    try:
        req = urllib.request.Request(BASE + "/api/v1/rooms/create", data=b"{}", headers={"Content-Type":"application/json"})
        with urllib.request.urlopen(req, timeout=1) as r:
            d = json.loads(r.read())
            code_lbl.config(text=f"Code: {d.get('code','?')}")
    except Exception as e:
        code_lbl.config(text=f"Error: {e}")

root = tk.Tk(); root.title("FIFA22 Companion (Alt C)"); root.geometry("320x160")
tk.Label(root, text="External Companion — No dxgi.dll").pack()
code_lbl = tk.Label(root, text="Code: ---", font=("Arial",14,"bold"))
code_lbl.pack()
status_lbl = tk.Label(root, text="Status: ---")
status_lbl.pack()
tk.Button(root, text="Create Room", command=create).pack(side=tk.LEFT, padx=5)
tk.Button(root, text="Refresh", command=fetch_status).pack(side=tk.LEFT, padx=5)
fetch_status()
root.mainloop()

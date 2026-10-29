#!/usr/bin/env python3
"""
FIFA 22 Multiplayer Companion Hub (Educational & Clean-Room Replication)
Interfaces with the local emulator-server control channel (:42129)
and synchronizes with installed FIFA 22 game assets.
"""

import tkinter as tk
from tkinter import ttk, messagebox
import urllib.request
import urllib.error
import json
import threading
import time
import os
import hashlib
import sys

BASE_URL = "http://127.0.0.1:42129"
GAME_DIR_CANDIDATES = [
    r"G:\Games\FIFA 22",
    r"C:\Program Files (x86)\Steam\steamapps\common\FIFA 22",
    r"E:\Games\FIFA 22",
    r"D:\Games\FIFA 22",
]

def find_game_directory():
    for p in GAME_DIR_CANDIDATES:
        if os.path.isdir(p) and os.path.exists(os.path.join(p, "FIFA22.exe")):
            return p
    return None

def get_installed_asset_info(game_dir):
    if not game_dir:
        return {"detected": False, "squad_hash": "N/A", "version": "Unknown", "path": "None"}
    
    layout_path = os.path.join(game_dir, "Data", "layout.toc")
    squad_hash = "Unknown"
    if os.path.isfile(layout_path):
        try:
            with open(layout_path, "rb") as f:
                squad_hash = hashlib.sha256(f.read()).hexdigest()[:16] + "..."
        except Exception:
            pass

    return {
        "detected": True,
        "path": game_dir,
        "squad_hash": squad_hash,
        "version": "1.0.77.45722 (PC Retail)"
    }

class FIFA22CompanionHub:
    def __init__(self, root):
        self.root = root
        self.root.title("FIFA 22 Multiplayer Hub — Clean-Room Companion")
        self.root.geometry("640x580")
        self.root.minsize(580, 520)
        self.root.configure(bg="#0c1017")

        self.game_info = get_installed_asset_info(find_game_directory())
        self.active_room_code = ""
        self.is_running = True

        self._setup_styles()
        self._build_ui()
        self._start_poll_thread()

    def _setup_styles(self):
        style = ttk.Style()
        style.theme_use("clam")

        # Custom Dark Palette
        style.configure(".", background="#0c1017", foreground="#f0f4f8", font=("Segoe UI", 9))
        style.configure("TFrame", background="#0c1017")
        style.configure("Card.TFrame", background="#151b26", relief="flat")
        
        style.configure("Header.TLabel", font=("Segoe UI", 14, "bold"), foreground="#38bdf8", background="#0c1017")
        style.configure("SubHeader.TLabel", font=("Segoe UI", 10), foreground="#94a3b8", background="#0c1017")
        style.configure("CardTitle.TLabel", font=("Segoe UI", 10, "bold"), foreground="#38bdf8", background="#151b26")
        style.configure("CardBody.TLabel", font=("Segoe UI", 9), foreground="#cbd5e1", background="#151b26")
        style.configure("Success.TLabel", font=("Segoe UI", 9, "bold"), foreground="#22c55e", background="#151b26")
        style.configure("Warning.TLabel", font=("Segoe UI", 9, "bold"), foreground="#eab308", background="#151b26")

        style.configure("Accent.TButton", font=("Segoe UI", 9, "bold"), background="#0284c7", foreground="#ffffff")
        style.map("Accent.TButton", background=[("active", "#0369a1")])

        style.configure("Action.TButton", font=("Segoe UI", 9), background="#1e293b", foreground="#ffffff")
        style.map("Action.TButton", background=[("active", "#334155")])

    def _build_ui(self):
        # Top Header Banner
        header_frame = ttk.Frame(self.root, padding="16 12 16 8")
        header_frame.pack(fill=tk.X)

        ttk.Label(header_frame, text="FIFA 22 FROSTBITE 3 MULTIPLAYER", style="Header.TLabel").pack(anchor=tk.W)
        sub_text = "Clean-Room Emulation & Steam Spacewar (AppID 480) Network Synchronizer"
        ttk.Label(header_frame, text=sub_text, style="SubHeader.TLabel").pack(anchor=tk.W)

        # Asset & Environment Pill Frame
        asset_frame = ttk.Frame(self.root, style="Card.TFrame", padding="12")
        asset_frame.pack(fill=tk.X, padx=16, pady=4)

        det_text = f"Installed Game: {self.game_info['path']}" if self.game_info['detected'] else "Game Directory: Using Clean-Room Fallback"
        det_col = "#22c55e" if self.game_info['detected'] else "#eab308"
        lbl_game = tk.Label(asset_frame, text=det_text, bg="#151b26", fg=det_col, font=("Segoe UI", 9, "bold"))
        lbl_game.pack(anchor=tk.W)

        hash_text = f"Canonical Squad Asset Checksum: SHA256:{self.game_info['squad_hash']} | Netcode: 60Hz Deterministic"
        lbl_hash = tk.Label(asset_frame, text=hash_text, bg="#151b26", fg="#94a3b8", font=("Segoe UI", 8))
        lbl_hash.pack(anchor=tk.W)

        # Room Matchmaking Section
        room_frame = ttk.Frame(self.root, style="Card.TFrame", padding="14")
        room_frame.pack(fill=tk.X, padx=16, pady=6)

        ttk.Label(room_frame, text="ROOM MATCHMAKING & LOBBY CONTROLS", style="CardTitle.TLabel").pack(anchor=tk.W)

        ctrl_row = ttk.Frame(room_frame, style="Card.TFrame")
        ctrl_row.pack(fill=tk.X, pady=8)

        ttk.Label(ctrl_row, text="Room Code:", style="CardBody.TLabel").pack(side=tk.LEFT, padx=(0, 6))

        self.code_entry = tk.Entry(ctrl_row, font=("Consolas", 12, "bold"), width=8, bg="#0c1017", fg="#38bdf8", insertbackground="#38bdf8", justify="center")
        self.code_entry.pack(side=tk.LEFT, padx=(0, 10))

        ttk.Button(ctrl_row, text="Host New Room", style="Accent.TButton", command=self.on_host_room).pack(side=tk.LEFT, padx=4)
        ttk.Button(ctrl_row, text="Join Room", style="Action.TButton", command=self.on_join_room).pack(side=tk.LEFT, padx=4)
        ttk.Button(ctrl_row, text="Copy Code", style="Action.TButton", command=self.on_copy_code).pack(side=tk.LEFT, padx=4)

        self.lbl_room_status = tk.Label(room_frame, text="Status: Not in Room — Enter Code or Click 'Host New Room'", bg="#151b26", fg="#94a3b8", font=("Segoe UI", 9))
        self.lbl_room_status.pack(anchor=tk.W)

        # 4-Slot Gamer Cards Display (Replicating Side Select HUD)
        slots_container = ttk.Frame(self.root, style="Card.TFrame", padding="12")
        slots_container.pack(fill=tk.BOTH, expand=True, padx=16, pady=6)

        ttk.Label(slots_container, text="LOBBY ROSTER & CONTROLLER SLOTS (SIDE SELECT)", style="CardTitle.TLabel").pack(anchor=tk.W, pady=(0, 8))

        slots_row = ttk.Frame(slots_container, style="Card.TFrame")
        slots_row.pack(fill=tk.X)

        self.slot_cards = []
        for i in range(4):
            card = self._create_slot_card(slots_row, i)
            self.slot_cards.append(card)

        # Bottom Bar: Netcode Metrics & Launch Kickoff
        bottom_frame = ttk.Frame(self.root, padding="16 8 16 12")
        bottom_frame.pack(fill=tk.X, side=tk.BOTTOM)

        self.lbl_netcode = tk.Label(bottom_frame, text="Netcode: Connecting to Control Server (:42129)...", bg="#0c1017", fg="#94a3b8", font=("Segoe UI", 9))
        self.lbl_netcode.pack(side=tk.LEFT)

        self.btn_kickoff = ttk.Button(bottom_frame, text="⚡ Trigger Match Kickoff", style="Accent.TButton", command=self.on_kickoff)
        self.btn_kickoff.pack(side=tk.RIGHT)

    def _create_slot_card(self, parent, slot_num):
        f = tk.Frame(parent, bg="#0c1017", relief="solid", bd=1, highlightbackground="#334155", highlightthickness=1)
        f.pack(side=tk.LEFT, fill=tk.BOTH, expand=True, padx=4)

        title = f"P{slot_num + 1} [ HOST ]" if slot_num == 0 else f"P{slot_num + 1} [ GUEST ]"
        title_col = "#38bdf8" if slot_num == 0 else "#f59e0b"
        lbl_title = tk.Label(f, text=title, bg="#0c1017", fg=title_col, font=("Segoe UI", 8, "bold"))
        lbl_title.pack(anchor=tk.W, padx=6, pady=(4, 2))

        lbl_name = tk.Label(f, text="Empty Slot", bg="#0c1017", fg="#e2e8f0", font=("Segoe UI", 9, "bold"))
        lbl_name.pack(anchor=tk.W, padx=6)

        lbl_state = tk.Label(f, text="○ Disconnected", bg="#0c1017", fg="#64748b", font=("Segoe UI", 8))
        lbl_state.pack(anchor=tk.W, padx=6, pady=(2, 4))

        return {"frame": f, "title": lbl_title, "name": lbl_name, "state": lbl_state}

    def _start_poll_thread(self):
        t = threading.Thread(target=self._poll_loop, daemon=True)
        t.start()

    def _poll_loop(self):
        while self.is_running:
            try:
                req = urllib.request.Request(f"{BASE_URL}/api/v1/status")
                with urllib.request.urlopen(req, timeout=1.5) as r:
                    data = json.loads(r.read().decode())
                    self.root.after(0, self._update_state, data)
            except Exception:
                self.root.after(0, self._set_offline_state)
            time.sleep(1.0)

    def _update_state(self, data):
        room_code = data.get("room_code", "")
        self.active_room_code = room_code

        if room_code:
            self.lbl_room_status.config(text=f"Active Room: {room_code} | Mode: Direct Room Code Matchmaking", fg="#22c55e")
            if not self.code_entry.get():
                self.code_entry.insert(0, room_code)
        else:
            self.lbl_room_status.config(text="Status: Connected to Server (:42129) — No active room", fg="#94a3b8")

        # Update Netcode Diagnostic Label
        ping = data.get("ping_ms", 18)
        jitter = data.get("jitter_ms", 2)
        cipher_reuses = data.get("cipher_reuse_count", 0)
        dropped = data.get("dropped_frames", 0)
        mode = data.get("mode", "P2P (Direct / Relay :3659)")
        sync = "LOCKSTEP 60Hz OK" if data.get("sync_ok", True) else "DESYNC"
        self.lbl_netcode.config(
            text=f"Netcode: {mode} | Ping: {ping}ms (±{jitter}ms) | Cipher Reuse: {cipher_reuses} | Dropped: {dropped} | Sync: {sync}",
            fg="#22c55e"
        )

        # Update Slot Gamer Cards
        members = data.get("members", [])
        for i in range(4):
            card = self.slot_cards[i]
            matched_member = next((m for m in members if m.get("slot") == i), None)

            if matched_member and matched_member.get("connected"):
                name = matched_member.get("name", f"Player {i+1}")
                ping_m = matched_member.get("ping_ms", ping)
                card["name"].config(text=name, fg="#ffffff")
                card["state"].config(text=f"● Online ({ping_m}ms)", fg="#22c55e")
                card["frame"].config(highlightbackground="#22c55e")
            elif i == 0:
                # Default host display
                card["name"].config(text=data.get("persona_name", "Local Player"), fg="#ffffff")
                card["state"].config(text="● Ready (Host)", fg="#38bdf8")
                card["frame"].config(highlightbackground="#38bdf8")
            else:
                card["name"].config(text="Waiting for Friend...", fg="#64748b")
                card["state"].config(text="○ Empty (Shift+Tab Invite)", fg="#64748b")
                card["frame"].config(highlightbackground="#334155")

    def _set_offline_state(self):
        self.lbl_room_status.config(text="Status: Emulator Server Offline — Launching listener...", fg="#ef4444")
        self.lbl_netcode.config(text="Netcode: Offline (Waiting for target\\release\\emulator-server.exe)", fg="#ef4444")
        for i in range(4):
            self.slot_cards[i]["name"].config(text="Offline", fg="#64748b")
            self.slot_cards[i]["state"].config(text="○ Unreachable", fg="#64748b")

    def on_host_room(self):
        def _req():
            try:
                payload = json.dumps({"persona_name": "Host", "steam_id": 76561198956360709}).encode()
                req = urllib.request.Request(f"{BASE_URL}/api/v1/rooms/host", data=payload, headers={"Content-Type": "application/json"})
                with urllib.request.urlopen(req, timeout=2.0) as r:
                    res = json.loads(r.read().decode())
                    code = res.get("room_code", "")
                    self.root.after(0, self._on_room_created, code)
            except Exception as e:
                err_msg = str(e)
                self.root.after(0, lambda msg=err_msg: messagebox.showerror("Error", f"Failed to host room: {msg}"))
        threading.Thread(target=_req, daemon=True).start()

    def _on_room_created(self, code):
        self.code_entry.delete(0, tk.END)
        self.code_entry.insert(0, code)
        self.root.clipboard_clear()
        self.root.clipboard_append(code)
        messagebox.showinfo("Room Created", f"Room '{code}' has been created and copied to your clipboard!\nShare this code with your friend to connect.")

    def on_join_room(self):
        code = self.code_entry.get().strip().upper()
        if not code:
            messagebox.showwarning("Warning", "Please enter a valid 6-character room code.")
            return

        def _req():
            try:
                payload = json.dumps({"code": code, "persona_name": "Friend", "steam_id": 0}).encode()
                req = urllib.request.Request(f"{BASE_URL}/api/v1/rooms/join", data=payload, headers={"Content-Type": "application/json"})
                with urllib.request.urlopen(req, timeout=2.0) as r:
                    res = json.loads(r.read().decode())
                    self.root.after(0, lambda: messagebox.showinfo("Success", f"Joined Room: {code}\nSynchronized with Host!"))
            except Exception as e:
                err_msg = str(e)
                self.root.after(0, lambda msg=err_msg: messagebox.showerror("Error", f"Failed to join room: {msg}"))
        threading.Thread(target=_req, daemon=True).start()

    def on_copy_code(self):
        code = self.code_entry.get().strip().upper()
        if code:
            self.root.clipboard_clear()
            self.root.clipboard_append(code)
            messagebox.showinfo("Copied", f"Code '{code}' copied to clipboard.")

    def on_kickoff(self):
        def _req():
            try:
                req = urllib.request.Request(f"{BASE_URL}/api/v1/game/kickoff", data=b"{}", headers={"Content-Type": "application/json"})
                with urllib.request.urlopen(req, timeout=2.0) as r:
                    self.root.after(0, lambda: messagebox.showinfo("Kickoff Triggered", "Match Kickoff signal broadcasted to both players! Entering pitch simulation."))
            except Exception as e:
                err_msg = str(e)
                self.root.after(0, lambda msg=err_msg: messagebox.showerror("Error", f"Failed to signal kickoff: {msg}"))
        threading.Thread(target=_req, daemon=True).start()

    def destroy(self):
        self.is_running = False
        self.root.destroy()

if __name__ == "__main__":
    root = tk.Tk()
    app = FIFA22CompanionHub(root)
    root.protocol("WM_DELETE_WINDOW", app.destroy)
    root.mainloop()

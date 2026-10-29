# Frostbite 3 & Battlefield 4 Multiplayer Architecture Reference

## 1. Core Summary
* **The Baseline:** Battlefield 4 (BF4) is the definitive multiplayer blueprint for Frostbite 3. The engine's core shooter logic, physics synchronization, and netcode were co-engineered explicitly for BF4's 64-player matches.
* **Direct IP Limitations:** Retail BF4 **does not** support direct-IP connections or native listen servers out-of-the-box. Connections are gated behind external backend/authentication systems.

## 2. Key Multi-Player Alternatives
* **Star Wars Battlefront I & II:** Asymmetrical sci-fi, 3rd/1st person hybrids, and linear narrative-competitive structures.
* **Plants vs. Zombies: Garden Warfare:** Class-based hero shooters and arena netcode.
* **Need for Speed: Rivals:** Dynamic open-world merging ("AllDrive" seamless MP).

## 3. Critical Technical Factors for Implementation
* **Client-Server Rigidity:** Frostbite 3 relies heavily on dedicated headless server builds (`Server.exe`) rather than client-hosted listen servers. 
* **The Authentication Gate:** Vanilla Frostbite requires a master server (e.g., Battlelog/EA Backend) to authenticate licenses and pass launch arguments to clients. There is no native `connect [IP]` console command.
* **Networked Destruction (Levolution):** High-level synchronization is required for real-time physics and large-scale environmental terrain shifts across all active clients.
* **Community Workarounds:** To achieve Direct-IP or LAN hosting, community projects utilize custom server emulator binaries (e.g., ZLOEmu) or leaked development builds to bypass the master server check.

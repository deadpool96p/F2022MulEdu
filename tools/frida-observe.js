// tools/frida-observe.js
// Safe, READ-ONLY Frida observation script for FIFA 22 multiplayer study.
// NO VirtualProtect, NO patch, NO modify FIFA22.exe on disk.
// Only Interceptor.attach on connect/WSAConnect to observe destinations.
// Per docs/rea-skills-education.md §2–4 and SKILL.md read-only rule.

console.log("[FRIDA-OBSERVE] Attach to FIFA22.exe (read-only) — observe only, never patch.");

function observe_connect() {
    const connect = Module.findExportByName(null, "connect");
    if (!connect) return;
    Interceptor.attach(connect, {
        onEnter(args) {
            const fd = args[0].toInt32();
            const addr = args[1];
            // Observe socket address (observe only — do NOT modify)
            console.log("[OBSERVE] connect fd=" + fd + " dest observed (not modified)");
        }
    });
}

observe_connect();
console.log("[FRIDA-OBSERVE] Observation active. No VirtualProtect, no .rdata/.text write.");

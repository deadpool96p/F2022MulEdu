import ctypes
import ctypes.wintypes
import time

PAGE_READWRITE = 0x04
FILE_MAP_READ = 0x04
EVENT_MODIFY_STATE = 0x0002
SYNCHRONIZE = 0x00100000

kernel32 = ctypes.windll.kernel32

def listen_debug_strings(duration_sec=5, target_pid=None):
    try:
        kernel32.CreateFileMappingW.argtypes = [ctypes.c_void_p, ctypes.c_void_p, ctypes.wintypes.DWORD, ctypes.wintypes.DWORD, ctypes.wintypes.DWORD, ctypes.c_wchar_p]
        kernel32.CreateFileMappingW.restype = ctypes.c_void_p
        kernel32.MapViewOfFile.argtypes = [ctypes.c_void_p, ctypes.wintypes.DWORD, ctypes.wintypes.DWORD, ctypes.wintypes.DWORD, ctypes.c_size_t]
        kernel32.MapViewOfFile.restype = ctypes.c_void_p

        buffer_ready = kernel32.CreateEventW(None, False, True, "DBWIN_BUFFER_READY")
        data_ready = kernel32.CreateEventW(None, False, False, "DBWIN_DATA_READY")
        file_mapping = kernel32.CreateFileMappingW(-1, None, PAGE_READWRITE, 0, 4096, "DBWIN_BUFFER")
        if not file_mapping:
            print("Failed to create DBWIN_BUFFER mapping (another debugger might be running).")
            return
        
        p_buf = kernel32.MapViewOfFile(file_mapping, FILE_MAP_READ, 0, 0, 4096)
        if not p_buf:
            print("Failed to map view of file.")
            return

        print(f"[*] Listening for Windows OutputDebugString for {duration_sec}s (Target PID: {target_pid})...")
        start = time.time()
        while time.time() - start < duration_sec:
            # Signal ready
            kernel32.SetEvent(buffer_ready)
            # Wait for data with 500ms timeout
            res = kernel32.WaitForSingleObject(data_ready, 500)
            if res == 0: # WAIT_OBJECT_0
                pid = ctypes.c_ulong.from_address(p_buf).value
                msg = ctypes.string_at(p_buf + 4)
                try:
                    text = msg.decode('latin1').strip()
                except Exception:
                    text = str(msg)
                if target_pid is None or pid == target_pid or "Blaze" in text or "Dirty" in text or "FIFA" in text:
                    print(f"[{pid}] {text}")
    finally:
        pass

if __name__ == "__main__":
    import subprocess
    target_pid = None
    try:
        out = subprocess.check_output('tasklist /fi "imagename eq FIFA22.exe"', shell=True).decode()
        for line in out.splitlines():
            if "FIFA22.exe" in line:
                parts = line.split()
                if len(parts) >= 2 and parts[1].isdigit():
                    target_pid = int(parts[1])
                    print(f"[*] Found FIFA22.exe with PID {target_pid}")
                    break
    except Exception as e:
        pass

    listen_debug_strings(120, target_pid=target_pid)

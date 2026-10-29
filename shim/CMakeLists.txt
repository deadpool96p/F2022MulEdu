cmake_minimum_required(VERSION 3.20)
project(seamless_fifa LANGUAGES C CXX ASM)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

include_directories(
    vendor/minhook/include
    vendor/blake3
    vendor/imgui
    vendor/imgui/backends
    src
)

# MinHook sources
set(MINHOOK_SOURCES
    vendor/minhook/src/buffer.c
    vendor/minhook/src/hook.c
    vendor/minhook/src/trampoline.c
    vendor/minhook/src/hde/hde64.c
    vendor/minhook/src/hde/hde32.c
)

# BLAKE3 with SIMD assembly sources
set(BLAKE3_SOURCES
    vendor/blake3/blake3.c
    vendor/blake3/blake3_dispatch.c
    vendor/blake3/blake3_portable.c
    vendor/blake3/blake3_sse2_x86-64_windows_gnu.S
    vendor/blake3/blake3_sse41_x86-64_windows_gnu.S
    vendor/blake3/blake3_avx2_x86-64_windows_gnu.S
    vendor/blake3/blake3_avx512_x86-64_windows_gnu.S
)

# ImGui sources (DirectX 12)
set(IMGUI_SOURCES
    vendor/imgui/imgui.cpp
    vendor/imgui/imgui_draw.cpp
    vendor/imgui/imgui_tables.cpp
    vendor/imgui/imgui_widgets.cpp
    vendor/imgui/backends/imgui_impl_win32.cpp
    vendor/imgui/backends/imgui_impl_dx12.cpp
)

add_library(seamless_fifa SHARED
    src/seamless_fifa.cpp
    src/overlay.cpp
    ${MINHOOK_SOURCES}
    ${BLAKE3_SOURCES}
    ${IMGUI_SOURCES}
)

target_link_libraries(seamless_fifa PRIVATE
    ws2_32
    d3d11
    d3d12
    dxgi
    d3dcompiler
    dwmapi
    user32
    kernel32
    crypt32
    bcrypt
    psapi
)

target_link_options(seamless_fifa PRIVATE -static -static-libgcc -static-libstdc++)

set_target_properties(seamless_fifa PROPERTIES
    OUTPUT_NAME "seamless_fifa"
    PREFIX ""
)

# DXGI Proxy Target (Dedicated clean forwarder & seamless_fifa.dll bootstrapper)
add_library(dxgi_shim SHARED
    src/dxgi_proxy.cpp
    dxgi.def
)

target_link_libraries(dxgi_shim PRIVATE
    kernel32
)

target_link_options(dxgi_shim PRIVATE -static)

set_target_properties(dxgi_shim PROPERTIES
    OUTPUT_NAME "dxgi"
    PREFIX ""
)

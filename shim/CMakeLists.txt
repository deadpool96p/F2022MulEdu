cmake_minimum_required(VERSION 3.20)
project(f22_shim C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# MinHook Static Library
set(MINHOOK_SOURCES
    vendor/minhook/src/buffer.c
    vendor/minhook/src/hook.c
    vendor/minhook/src/trampoline.c
    vendor/minhook/src/hde/hde64.c
    vendor/minhook/src/hde/hde32.c
)

add_library(minhook STATIC ${MINHOOK_SOURCES})
target_include_directories(minhook PUBLIC vendor/minhook/include)

# Dear ImGui Sources
set(IMGUI_SOURCES
    vendor/imgui/imgui.cpp
    vendor/imgui/imgui_draw.cpp
    vendor/imgui/imgui_tables.cpp
    vendor/imgui/imgui_widgets.cpp
    vendor/imgui/backends/imgui_impl_win32.cpp
    vendor/imgui/backends/imgui_impl_dx11.cpp
    vendor/imgui/backends/imgui_impl_dx12.cpp
)

# DXGI Proxy DLL
set(SHIM_SOURCES
    src/dllmain.cpp
    src/logger.cpp
    src/config.cpp
    src/hooks_winsock.cpp
    src/cert_bypass.cpp
    src/control_client.cpp
    src/hooks_dxgi.cpp
    src/overlay/input_hook.cpp
    src/overlay/minimal_overlay.cpp
    ${IMGUI_SOURCES}
    dxgi.def
)

add_library(dxgi_proxy SHARED ${SHIM_SOURCES})

target_include_directories(dxgi_proxy PRIVATE
    include
    vendor/minhook/include
    vendor/imgui
    vendor/imgui/backends
    vendor/steam/include
)

target_link_libraries(dxgi_proxy PRIVATE
    minhook
    ws2_32
    crypt32
    psapi
    d3d11
    d3d12
    d3dcompiler
    dxgi
    dwmapi
    winhttp
    xinput
)

target_link_options(dxgi_proxy PRIVATE
    -static
    -static-libgcc
    -static-libstdc++
)

# Output dxgi.dll without "lib" prefix
set_target_properties(dxgi_proxy PROPERTIES
    PREFIX ""
    OUTPUT_NAME "dxgi"
)

# Post-build: Copy steam_api64.dll and generate steam_appid.txt in build directory
add_custom_command(TARGET dxgi_proxy POST_BUILD
    COMMAND ${CMAKE_COMMAND} -E copy_if_different
        "${CMAKE_CURRENT_SOURCE_DIR}/vendor/steam/bin/win64/steam_api64.dll"
        "${CMAKE_CURRENT_BINARY_DIR}/steam_api64.dll"
    COMMAND ${CMAKE_COMMAND} -E echo "480" > "${CMAKE_CURRENT_BINARY_DIR}/steam_appid.txt"
    COMMENT "Copying Steamworks runtime dependencies (steam_api64.dll & steam_appid.txt: 480)"
)

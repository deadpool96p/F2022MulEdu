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

# DXGI Proxy DLL
set(SHIM_SOURCES
    src/dllmain.cpp
    src/logger.cpp
    src/config.cpp
    src/hooks_winsock.cpp
    src/cert_bypass.cpp
    dxgi.def
)

add_library(dxgi_proxy SHARED ${SHIM_SOURCES})

target_include_directories(dxgi_proxy PRIVATE
    include
    vendor/minhook/include
)

target_link_libraries(dxgi_proxy PRIVATE
    minhook
    ws2_32
    crypt32
    psapi
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

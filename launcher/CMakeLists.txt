cmake_minimum_required(VERSION 3.20)
project(FIFA22_Seamless LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

add_executable(FIFA22_Seamless
    src/main.cpp
    src/ini_parser.h
)

target_link_libraries(FIFA22_Seamless PRIVATE
    kernel32
    user32
)

set_target_properties(FIFA22_Seamless PROPERTIES
    OUTPUT_NAME "FIFA22_Seamless"
)

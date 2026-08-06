cmake_minimum_required(VERSION 3.14)
project(LogosAgentPlugin LANGUAGES CXX)

if(DEFINED ENV{LOGOS_MODULE_BUILDER_ROOT})
    include($ENV{LOGOS_MODULE_BUILDER_ROOT}/cmake/LogosModule.cmake)
else()
    message(FATAL_ERROR "LogosModule.cmake not found. Set LOGOS_MODULE_BUILDER_ROOT.")
endif()

configure_file(${CMAKE_CURRENT_SOURCE_DIR}/metadata.json
               ${CMAKE_CURRENT_BINARY_DIR}/metadata.json COPYONLY)

logos_module(
    NAME agent
    SOURCES
        src/agent_interface.h
        src/agent_ffi_client.h
        src/agent_ffi_client.cpp
        src/agent_plugin.h
        src/agent_plugin.cpp
)

cmake_minimum_required(VERSION 3.20)

project(binaryninja-core)

set(_triton_bn_binaryninja_git_tag "stable/5.2.8722")

include(FetchContent)
FetchContent_Declare(binaryninjaapi
    GIT_REPOSITORY https://github.com/Vector35/binaryninja-api.git
    GIT_TAG        ${_triton_bn_binaryninja_git_tag}
)
FetchContent_GetProperties(binaryninjaapi)
if(NOT binaryninjaapi_POPULATED)
    FetchContent_Populate(binaryninjaapi)  # For binaryninjaapi_SOURCE_DIR
endif()

add_library(binaryninjacore SHARED
    binaryninjacore.cc
)
set_target_properties(binaryninjacore PROPERTIES
    SOVERSION 1
)
target_include_directories(binaryninjacore PRIVATE
    "${binaryninjaapi_SOURCE_DIR}"
)

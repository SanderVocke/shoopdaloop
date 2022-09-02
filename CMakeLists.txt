cmake_minimum_required(VERSION 3.22)
project(shoopdaloop_backend)

set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}" "${CMAKE_SOURCE_DIR}/cmake")

add_subdirectory(backend)
add_subdirectory(shoopdaloop/lib/shiboken)
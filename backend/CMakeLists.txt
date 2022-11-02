cmake_minimum_required(VERSION 3.22)
project(shoopdaloop_backend)

# Set up language settings
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_SOURCE_DIR}")

# Find Halide
find_package(Halide REQUIRED)
find_package(Jack REQUIRED)

# Generator
add_halide_generator(shoopdaloop_loops.generator SOURCES loops_generator.cpp)
add_halide_generator(sine.generator SOURCES sine_generator.cpp)

# Filters
add_halide_library(shoopdaloop_loops 
    FROM shoopdaloop_loops.generator
    TARGETS host-profile
    )
add_halide_library(sine FROM sine.generator)

# Backend lib
add_library(shoopdaloop_backend SHARED shoopdaloop_backend.cpp)
target_link_libraries(shoopdaloop_backend PRIVATE
                      Halide::Tools
                      Halide
                      shoopdaloop_loops
                      ${JACK_LIBRARIES})
target_include_directories(shoopdaloop_backend
                        PRIVATE
                        ${JACK_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_BINARY_DIR}
                        )

add_executable(shoopdaloop_backend_test shoopdaloop_backend_test.cpp)
target_compile_options(shoopdaloop_backend_test PRIVATE $<$<CXX_COMPILER_ID:GNU,Clang,AppleClang>:-O2>)
target_link_libraries(shoopdaloop_backend_test
                      PRIVATE
                      shoopdaloop_backend)
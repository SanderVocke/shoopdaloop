cmake_minimum_required(VERSION 3.22)
project(shoopdaloop_backend)

# Set up language settings
set(CMAKE_CXX_STANDARD 17)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)

# Find Halide
find_package(Halide REQUIRED)
find_package(OpenMP)

# Generator
add_halide_generator(shoopdaloop_loops.generator SOURCES loops_generator.cpp)

# Filters
add_halide_library(shoopdaloop_loops FROM shoopdaloop_loops.generator)

# Main executable
add_executable(shoopdaloop_loops_test test.cpp)
target_compile_options(shoopdaloop_loops_test PRIVATE $<$<CXX_COMPILER_ID:GNU,Clang,AppleClang>:-O2>)
target_link_libraries(shoopdaloop_loops_test
                      PRIVATE
                      Halide::Tools
                      shoopdaloop_loops
                      $<TARGET_NAME_IF_EXISTS:OpenMP::OpenMP_CXX>)

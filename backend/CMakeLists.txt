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

# Filters
add_halide_library(shoopdaloop_loops 
    FROM shoopdaloop_loops.generator
    TARGETS host-profile
    )

# Main executable
add_executable(shoopdaloop_loops_test test.cpp)
target_compile_options(shoopdaloop_loops_test PRIVATE $<$<CXX_COMPILER_ID:GNU,Clang,AppleClang>:-O2>)
target_link_libraries(shoopdaloop_loops_test
                      PRIVATE
                      Halide::Tools
                      shoopdaloop_loops
                      $<TARGET_NAME_IF_EXISTS:OpenMP::OpenMP_CXX>)

add_executable(shoopdaloop_jack jack.cpp)
target_compile_options(shoopdaloop_jack PRIVATE $<$<CXX_COMPILER_ID:GNU,Clang,AppleClang>:-O2>)
target_link_libraries(shoopdaloop_jack
                      PRIVATE
                      Halide::Tools
                      shoopdaloop_loops
                      $<TARGET_NAME_IF_EXISTS:OpenMP::OpenMP_CXX>
                      ${JACK_LIBRARIES})
target_include_directories(shoopdaloop_jack
                        PRIVATE
                        ${JACK_INCLUDE_DIRS}
                        )

add_executable(simple_client simple_client.c)
target_compile_options(simple_client PRIVATE $<$<CXX_COMPILER_ID:GNU,Clang,AppleClang>:-O2>)
target_link_libraries(simple_client
                      PRIVATE
                      ${JACK_LIBRARIES})
target_include_directories(simple_client
                        PRIVATE
                        ${JACK_INCLUDE_DIRS}
                        )
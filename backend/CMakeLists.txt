# Set up language settings
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}")

# Find Halide
find_package(Halide REQUIRED)
find_package(Jack REQUIRED)
find_package(Boost COMPONENTS ALL REQUIRED)

# Generator
add_halide_generator(shoopdaloop_loops.generator SOURCES loops_generator.cpp)

# Filters
add_halide_library(shoopdaloop_loops 
    FROM shoopdaloop_loops.generator
    TARGETS host
    )
add_halide_library(shoopdaloop_loops_profile
    GENERATOR shoopdaloop_loops
    FUNCTION_NAME shoopdaloop_loops_profile
    FROM shoopdaloop_loops.generator
    TARGETS host-profile
    )
add_halide_library(shoopdaloop_loops_trace
    GENERATOR shoopdaloop_loops
    FUNCTION_NAME shoopdaloop_loops_trace
    FROM shoopdaloop_loops.generator
    TARGETS host-trace_stores
    )

# Backend lib
add_library(shoopdaloop_backend SHARED shoopdaloop_backend.cpp)
target_link_libraries(shoopdaloop_backend PRIVATE
                      Halide::Tools
                      Halide
                      shoopdaloop_loops
                      shoopdaloop_loops_profile
                      ${BOOST_LIBRARIES}
                      ${JACK_LIBRARIES})
target_include_directories(shoopdaloop_backend
                        PRIVATE
                        ${JACK_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_BINARY_DIR}
                        ${BOOST_INCLUDE_DIRS}
                        )

# Tests
add_executable(loops_test loops_test.cpp)
target_link_libraries(loops_test PRIVATE
                      Halide::Tools
                      Halide
                      shoopdaloop_loops
                      shoopdaloop_loops_profile
                      shoopdaloop_loops_trace
                      ${BOOST_LIBRARIES}
                      ${JACK_LIBRARIES})
target_include_directories(loops_test
                        PRIVATE
                        ${JACK_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_BINARY_DIR}
                        ${BOOST_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_SOURCE_DIR}/../third_party/ut/include
                        )

# Autogen Python bindings
add_custom_target(
    __init__.py
    touch __init__.py
)
add_custom_target(
  shoopdaloop_backend.py
  ALL ctypesgen -lshoopdaloop_backend ${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop_backend.h -o shoopdaloop_backend.py
  DEPENDS shoopdaloop_backend __init__.py
  )
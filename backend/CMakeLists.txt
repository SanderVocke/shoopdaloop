# Set up language settings
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}")

# Find pkgs
find_package(Jack REQUIRED)
find_package(Boost COMPONENTS ALL REQUIRED)

# Includes
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/frontend_interface)

# Subdirs
add_subdirectory(test)
add_subdirectory(frontend_interface)
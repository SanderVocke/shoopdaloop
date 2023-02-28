# Set up language settings
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}" "${CMAKE_SOURCE_DIR}/cmake")

find_package(Qt6 REQUIRED COMPONENTS Core QuickTest)
qt_standard_project_setup()

# Options
option(ENABLE_PERFTOOLS "Link to pprof profiling library")
if(ENABLE_PERFTOOLS)
    add_compile_definitions(ENABLE_PERFTOOLS)
    find_package(Gperftools REQUIRED COMPONENTS profiler)
    link_libraries(${GPERFTOOLS_LIBRARIES})
    include_directories(${GPERFTOOLS_INCLUDES})
endif()

# Find pkgs
find_package(Jack REQUIRED)
find_package(Boost COMPONENTS ALL REQUIRED)

# Includes
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/frontend_interface)

# Subdirs
add_subdirectory(test)
add_subdirectory(frontend_interface)
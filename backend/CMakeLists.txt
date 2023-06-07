# Set up language settings
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)
set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}" "${CMAKE_SOURCE_DIR}/cmake")

find_package(Qt6 REQUIRED COMPONENTS Core QuickTest)
qt_standard_project_setup()

set(RUN_CMD_PREFIX "")

# Options
option(ENABLE_PERFTOOLS "Link to pprof profiling library")
if(ENABLE_PERFTOOLS)
    add_compile_definitions(ENABLE_PERFTOOLS)
    find_package(Gperftools REQUIRED COMPONENTS profiler)
    link_libraries(${GPERFTOOLS_LIBRARIES})
    include_directories(${GPERFTOOLS_INCLUDES})
endif()
option(ENABLE_ASAN "Enable address sanitizer")
if(ENABLE_ASAN)
    add_compile_options(-fsanitize=address -shared-libasan)
    add_link_options(-fsanitize=address -shared-libasan)
    if (CMAKE_CXX_COMPILER_ID STREQUAL "Clang")
        execute_process(COMMAND clang -print-file-name=libclang_rt.asan-x86_64.so OUTPUT_STRIP_TRAILING_WHITESPACE OUTPUT_VARIABLE ASAN_DSO)
        set(RUN_CMD_PREFIX LD_PRELOAD=${ASAN_DSO} ASAN_OPTIONS=detect_leaks=0 ${RUN_CMD_PREFIX})
    endif()
endif()

# Find pkgs
find_package(Jack REQUIRED)
find_package(LV2 REQUIRED)
find_package(Lilv REQUIRED)
find_package(Boost COMPONENTS ALL REQUIRED)
find_package(spdlog REQUIRED)
find_package(fmt REQUIRED)

message("${SPDLOG_LIBRARIES} ${FMT_LIBRARIES}")

# Includes
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/frontend_interface)

# Subdirs
add_subdirectory(${CMAKE_SOURCE_DIR}/third_party/lv2_evbuf lv2_evbuf)
add_subdirectory(test)
add_subdirectory(frontend_interface)
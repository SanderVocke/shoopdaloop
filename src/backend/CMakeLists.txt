cmake_minimum_required(VERSION 3.22)
set(CMAKE_OSX_ARCHITECTURES "${ARCHS_STANDARD}") # Enables a universal build on MacOS where possible
project(shoopdaloop_backend)

# Set up language settings
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)
set(CMAKE_EXPORT_COMPILE_COMMANDS YES)
set(CMAKE_VISIBILITY_INLINES_HIDDEN 1)

find_package(Threads REQUIRED)

set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}" "${CMAKE_SOURCE_DIR}/cmake")

# Ensure PDB file generation
if (WIN32)
  set(CMAKE_PDB_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/pdb")
  if (CMAKE_CXX_COMPILER_ID MATCHES "MSVC")
    add_compile_options("/Z7")
    add_compile_options("/FS")
    add_compile_options("/EHsc")
#    add_compile_options("-D_ITERATOR_DEBUG_LEVEL=0")
    add_link_options("/DEBUG")
    add_link_options("/OPT:REF")
    add_link_options("/OPT:ICF")
#    set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")
  elseif(CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    add_compile_options(-g -gcodeview)
    add_link_options(-fuse-ld=lld -g -Wl,--pdb=)
  endif()

  # Prevent access violations in STL mutex functions due to mixed
  # runtime DLLs in PyInstaller distribution
  add_compile_definitions(_DISABLE_CONSTEXPR_MUTEX_CONSTRUCTOR)
endif()

# Build ID for debug info on Gcc/Clang (unix)
if (UNIX)
  if(CMAKE_C_COMPILER_ID MATCHES "GNU|Clang")
      add_link_options(-Wl,--build-id)
  endif()
endif()

option(ENABLE_COVERAGE "Enable code coverage" OFF)
if(ENABLE_COVERAGE)
  include (CodeCoverage)
  append_coverage_compiler_flags()
endif()

option(BACKEND_JACK "Enable JACK audio backend" ON)
if(BACKEND_JACK)
  add_compile_definitions(SHOOP_HAVE_BACKEND_JACK)
endif()

option(HAVE_LV2 "Enable LV2 plugin hosting" ON)
if(HAVE_LV2)
  add_compile_definitions(SHOOP_HAVE_LV2)
endif()

if (APPLE)
  message(STATUS "MacOS architectures included: ${CMAKE_OSX_ARCHITECTURES}")
endif()

set(PREV_SETTING ${CMAKE_POSITION_INDEPENDENT_CODE})
set(CMAKE_POSITION_INDEPENDENT_CODE TRUE)
set(CMAKE_POSITION_INDEPENDENT_CODE ${PREV_SETTING})

find_package(Catch2 CONFIG REQUIRED)

include(CompilerCaching)

set(RUN_CMD_PREFIX "")

# Options
option(ENABLE_PERFTOOLS "Link to pprof profiling library")
if(ENABLE_PERFTOOLS)
    add_compile_definitions(ENABLE_PERFTOOLS)
    find_package(Gperftools REQUIRED COMPONENTS profiler)
    link_libraries(${GPERFTOOLS_LIBRARIES})
    include_directories(${GPERFTOOLS_INCLUDES})
endif()
option(ENABLE_ASAN "Enable address sanitizer" OFF)
if(ENABLE_ASAN)
    if (CMAKE_CXX_COMPILER_ID STREQUAL "Clang")
        add_compile_options(-fsanitize=address -shared-libasan)
        add_link_options(-fsanitize=address -shared-libasan)
        execute_process(COMMAND clang -print-file-name=libclang_rt.asan-x86_64.so OUTPUT_STRIP_TRAILING_WHITESPACE OUTPUT_VARIABLE ASAN_DSO)
    elseif(CMAKE_CXX_COMPILER_ID STREQUAL "GNU")
        add_compile_options(-fsanitize=address)
        add_link_options(-fsanitize=address)
        execute_process(COMMAND gcc -print-file-name=libasan.so OUTPUT_STRIP_TRAILING_WHITESPACE OUTPUT_VARIABLE ASAN_DSO)
    else()
        message(FATAL_ERROR "Address sanitizer is only supported on Clang and GCC")
    endif()
    if (EXISTS ${ASAN_DSO})
      file(READ ${ASAN_DSO} ASAN_DSO_CONTENT)
      string(FIND "${ASAN_DSO_CONTENT}" "GNU ld script" FOUND_LD_SCRIPT)
      if(NOT ${FOUND_LD_SCRIPT} EQUAL -1)
        # Pointed to a linker script, find the actual lib
        string(REGEX MATCH ".*INPUT[ ]*\\([ ]*([^\)^ ]+)[ ]*\\).*" FOUND_ASAN_SO ${ASAN_DSO_CONTENT})
        set(ASAN_DSO ${CMAKE_MATCH_1})
        message("Found that libasan.so is a linker script pointing to ${ASAN_DSO}.")
      endif()
    endif()
    set(RUN_CMD_PREFIX LD_PRELOAD=${ASAN_DSO} ${RUN_CMD_PREFIX})
    set(RUN_CMD_PREFIX ${RUN_CMD_PREFIX} PARENT_SCOPE)
    get_filename_component(ASAN_DSO_REALPATH ${ASAN_DSO} REALPATH)
    if(${CMAKE_BUILD_TYPE} STREQUAL "Debug")
      install(FILES ${ASAN_DSO_REALPATH}
            DESTINATION debug/lib)
    else()
      install(FILES ${ASAN_DSO_REALPATH}
            DESTINATION lib)
    endif()
endif()
option(ENABLE_PROCESS_PROFILING "Enable internal audio processing profiling" ON)
if(ENABLE_PROCESS_PROFILING)
    add_compile_options(-DSHOOP_PROFILING)
endif()
set(OVERRIDE_COMPILE_LOG_LEVEL "" CACHE STRING "If set, overrides the compile-time log level filter. Valid options are LOG_LEVEL_[TRACE, DEBUG, INFO, WARNING, ERROR].")
option(ENABLE_TRACKED_SHARED_PTRS "Enable tracked shared pointers for debugging")
if(ENABLE_TRACKED_SHARED_PTRS)
    message("Using tracked shared pointers for debugging.")
    add_compile_options(-DUSE_TRACKED_SHARED_PTRS)
endif()

# Needed for windows to get backtraces from Boost
if (WIN32)
add_compile_definitions(BOOST_STACKTRACE_USE_WINDBG)
link_libraries(dbgeng.lib)
elseif (APPLE)
add_compile_definitions(_GNU_SOURCE)
endif ()

# Find pkgs
# Variable to store the result (will be either "NEW", "OLD", or "NOTFOUND")
if(POLICY CMP0167)
    cmake_policy(SET CMP0167 NEW) # Use FindBoost as opposed to BoostConfig.cmake
endif()
if (WIN32)
  find_package(Boost REQUIRED headers system)
else()
  find_package(Boost REQUIRED headers)
endif()

if (BACKEND_JACK)
  # Generate a wrapper for libjack which allows dynamic loading using dlopen
  find_package(Jack REQUIRED)
  add_compile_definitions(SHOOP_HAVE_BACKEND_JACK)
endif()

# Generate the run cmd prefix so that it can be referred to after the build (e.g. when running tests)
add_custom_command(
  OUTPUT ${CMAKE_BINARY_DIR}/run_cmd_prefix.txt
  COMMAND echo ${RUN_CMD_PREFIX} > ${CMAKE_BINARY_DIR}/run_cmd_prefix.txt
  COMMENT "Storing run cmd prefix (${RUN_CMD_PREFIX}) into ${CMAKE_BINARY_DIR}/run_cmd_prefix.txt"
)
add_custom_target(
  run_cmd_prefix_file
  ALL
  DEPENDS ${CMAKE_BINARY_DIR}/run_cmd_prefix.txt
)

set(EXTERNAL_DEPS_PREFIX ${CMAKE_CURRENT_BINARY_DIR}/external_deps)

# Backend lib
file(GLOB INTERNAL_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/*.cpp)
if (HAVE_LV2)
  file(GLOB LV2_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/lv2/*.cpp)
  list(APPEND INTERNAL_SOURCES ${LV2_SOURCES}
       ${CMAKE_SOURCE_DIR}/third_party/lv2_evbuf/lv2_evbuf.c)
  include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal/lv2
                      ${CMAKE_SOURCE_DIR/third_party/lv2_evbuf})
  pkg_search_module(lilv REQUIRED IMPORTED_TARGET GLOBAL lilv-0)
  pkg_search_module(lv2 REQUIRED IMPORTED_TARGET GLOBAL lv2)
endif()
if (BACKEND_JACK)
  file(GLOB JACK_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/jack/*.cpp)
  list(APPEND INTERNAL_SOURCES ${JACK_SOURCES})
  include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal/jack)
  pkg_search_module(jack REQUIRED IMPORTED_TARGET GLOBAL jack)
endif()

# Find libzita-resampler
# Try to find the package normally (not REQUIRED)
find_package(zita-resampler QUIET)

find_package(fmt)

if(NOT zita-resampler_FOUND)
    message(WARNING "zita-resampler not found via find_package. Attempting manual search...")

    # Try to find the library manually
    find_library(ZITA_RESAMPLER_LIBRARY
        NAMES zita-resampler zita-resampler.so
        DOC "zita-resampler library"
    )

    # Try to find the include directory
    find_path(ZITA_RESAMPLER_INCLUDE_DIR
        NAMES zita-resampler/resampler.h
        DOC "zita-resampler include directory"
    )

    message(STATUS "Zita: ${ZITA_RESAMPLER_LIBRARY}, ${ZITA_RESAMPLER_INCLUDE_DIR}")

    # Check if both were found
    if(ZITA_RESAMPLER_LIBRARY AND ZITA_RESAMPLER_INCLUDE_DIR)
        message(STATUS "Found zita-resampler manually.")
        
        # Define an imported target to simulate a proper package
        add_library(zita-resampler::zita-resampler UNKNOWN IMPORTED)
        set_target_properties(zita-resampler::zita-resampler PROPERTIES
            IMPORTED_LOCATION "${ZITA_RESAMPLER_LIBRARY}"
            INTERFACE_INCLUDE_DIRECTORIES "${ZITA_RESAMPLER_INCLUDE_DIR}"
        )
    else()
        message(FATAL_ERROR "zita-resampler not found. Aborting.")
    endif()
endif()

# Includes
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal)
include_directories(${CMAKE_CURRENT_SOURCE_DIR})
include_directories(${CMAKE_SOURCE_DIR}/third_party/nlohmann_json)

add_subdirectory(test)

add_library(shoopdaloop_backend_internals STATIC libshoopdaloop_backend.cpp ${INTERNAL_SOURCES})
set_target_properties(shoopdaloop_backend_internals PROPERTIES POSITION_INDEPENDENT_CODE ON)
target_include_directories(shoopdaloop_backend_internals
                        PRIVATE
                        ${CMAKE_SOURCE_DIR}/third_party/base64/include
                        ${CMAKE_CURRENT_BINARY_DIR}
                        )
target_link_libraries(shoopdaloop_backend_internals PRIVATE
  Threads::Threads
  fmt::fmt
  zita-resampler::zita-resampler
  Boost::headers
  )
if (WIN32)
  target_link_libraries(shoopdaloop_backend_internals PRIVATE Boost::system)
endif()
if (UNIX)
    target_link_libraries(shoopdaloop_backend_internals PUBLIC dl)
endif()
if (HAVE_LV2)
  target_include_directories(shoopdaloop_backend_internals
                          PRIVATE
                          ${CMAKE_SOURCE_DIR}/third_party/lv2_evbuf
                          ${CMAKE_SOURCE_DIR}/third_party/lv2_external_ui
                          )
  target_link_libraries(shoopdaloop_backend_internals PUBLIC PkgConfig::lilv PkgConfig::lv2)
endif()
if (BACKEND_JACK)
  target_include_directories(shoopdaloop_backend_internals PRIVATE ${JACK_INCLUDE_DIRS})
endif()

file(WRITE ${CMAKE_CURRENT_BINARY_DIR}/empty.cpp "#include \"libshoopdaloop_backend.h\"")
add_library(shoopdaloop_backend SHARED ${CMAKE_CURRENT_BINARY_DIR}/empty.cpp)
target_link_libraries(shoopdaloop_backend PRIVATE
   "$<LINK_LIBRARY:WHOLE_ARCHIVE,shoopdaloop_backend_internals>"
   zita-resampler::zita-resampler
   Threads::Threads)
if (WIN32)
   target_link_libraries(shoopdaloop_backend PRIVATE Boost::system)
endif()
# create_breakpad_symbols_dump(shoopdaloop_backend)

# if(APPLE)
#   set_target_properties(shoopdaloop_backend PROPERTIES
#       BUILD_WITH_INSTALL_RPATH TRUE
#       INSTALL_RPATH "@loader_path/"  # Load dylibs from same directory
#   )
# elseif(UNIX)
#   set_target_properties(shoopdaloop_backend PROPERTIES
#     INSTALL_RPATH "$ORIGIN/"  # Load .so's from same directory
#   )
# endif()

# Use vcpkg-compatible installation structure
if(${CMAKE_BUILD_TYPE} STREQUAL "Debug")
  install(TARGETS shoopdaloop_backend
          DESTINATION debug/lib)
else()
  install(TARGETS shoopdaloop_backend
          DESTINATION lib)
endif()

if (OVERRIDE_COMPILE_LOG_LEVEL STREQUAL "")
  if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    message("Building in debug mode. Using compile time log level for back-end: LOG_LEVEL_DEBUG_TRACE")
    target_compile_definitions(shoopdaloop_backend_internals PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_DEBUG_TRACE)
  else()
    message("Building in non-debug mode. Disabling trace messages. Using compile time log level for back-end: LOG_LEVEL_ALWAYS_TRACE")
    target_compile_definitions(shoopdaloop_backend_internals PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_ALWAYS_TRACE)
  endif()
else()
  message(WARNING "Using overridden compile time log level for back-end: ${OVERRIDE_COMPILE_LOG_LEVEL}")
  target_compile_definitions(shoopdaloop_backend_internals PUBLIC -DCOMPILE_LOG_LEVEL=${OVERRIDE_COMPILE_LOG_LEVEL})
endif()

# Generate javascript copy of enums into source dir
set(PATH_SEP ":")
if (WIN32)
  set(PATH_SEP $<SEMICOLON>)
endif()
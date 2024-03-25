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
    install(FILES ${ASAN_DSO_REALPATH}
            EXCLUDE_FROM_ALL
            COMPONENT package_install
            DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
endif()
option(ENABLE_PROCESS_PROFILING "Enable internal audio processing profiling" ON)
if(ENABLE_PROCESS_PROFILING)
    add_compile_options(-DSHOOP_PROFILING)
endif()
set(OVERRIDE_COMPILE_LOG_LEVEL "" CACHE STRING "If set, overrides the compile-time log level filter. Valid options are LOG_LEVEL_[TRACE, DEBUG, INFO, WARNING, ERROR].")

# Needed for windows to get backtraces from Boost
if (WIN32)
add_compile_definitions(BOOST_STACKTRACE_USE_WINDBG)
link_libraries(dbgeng.lib)
elseif (APPLE)
add_compile_definitions(_GNU_SOURCE)
endif ()

# Find pkgs
find_package(Boost REQUIRED)

if (BACKEND_JACK)
  # Generate a wrapper for libjack which allows dynamic loading using dlopen
  find_package(Jack REQUIRED)
  add_compile_definitions(-DSHOOP_HAVE_BACKEND_JACK)
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

add_subdirectory(crashhandling)

# Backend lib
file(GLOB INTERNAL_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/*.cpp)
if (HAVE_LV2)
  include (${CMAKE_CURRENT_SOURCE_DIR}/BuildLV2Projects.cmake)
  file(GLOB LV2_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/lv2/*.cpp)
  list(APPEND INTERNAL_SOURCES ${LV2_SOURCES}
       ${CMAKE_SOURCE_DIR}/../third_party/lv2_evbuf/lv2_evbuf.c)
  include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal/lv2
                      ${CMAKE_SOURCE_DIR/../third_party/lv2_evbuf})
endif()
if (BACKEND_JACK)
  file(GLOB JACK_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/jack/*.cpp)
  list(APPEND INTERNAL_SOURCES ${JACK_SOURCES})
  include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal/jack)
endif()

# build libzita-resampler
include(${CMAKE_CURRENT_SOURCE_DIR}/BuildZitaResampler.cmake)

# Includes
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal)
include_directories(${CMAKE_CURRENT_SOURCE_DIR})
include_directories(${CMAKE_SOURCE_DIR}/../third_party/nlohmann_json)

add_subdirectory(test)
add_subdirectory(accelerate)

add_library(shoopdaloop_internals STATIC libshoopdaloop.cpp ${INTERNAL_SOURCES})
set_target_properties(shoopdaloop_internals PROPERTIES POSITION_INDEPENDENT_CODE ON)
target_include_directories(shoopdaloop_internals
                        PRIVATE
                        ${CMAKE_SOURCE_DIR}/../third_party/base64/include
                        ${CMAKE_CURRENT_BINARY_DIR}
                        ${Boost_INCLUDE_DIR}
                        )
target_link_libraries(shoopdaloop_internals PRIVATE
  Threads::Threads
  fmt::fmt
  zita
  )
if (UNIX)
    target_link_libraries(shoopdaloop_internals PUBLIC dl)
endif()
if (HAVE_LV2)
  target_include_directories(shoopdaloop_internals
                          PRIVATE
                          ${CMAKE_SOURCE_DIR}/../third_party/lv2_evbuf
                          ${LV2_INCLUDE_DIRS}
                          ${CMAKE_SOURCE_DIR}/../third_party/lv2_external_ui
                          )
  add_dependencies(shoopdaloop_internals lilv_proj)
endif()
if (BACKEND_JACK)
  target_include_directories(shoopdaloop_internals PRIVATE ${JACK_INCLUDE_DIRS})
endif()

file(WRITE ${CMAKE_CURRENT_BINARY_DIR}/empty.cpp "#include \"libshoopdaloop.h\"")
add_library(shoopdaloop SHARED ${CMAKE_CURRENT_BINARY_DIR}/empty.cpp)
target_link_libraries(shoopdaloop PRIVATE "$<LINK_LIBRARY:WHOLE_ARCHIVE,shoopdaloop_internals>")
create_breakpad_symbols_dump(shoopdaloop)

if(UNIX)
  set_target_properties(shoopdaloop PROPERTIES
    INSTALL_RPATH "$ORIGIN/"  # Load .so's from same directory
  )
elseif(APPLE)
  set_target_properties(shoopdaloop PROPERTIES
      BUILD_WITH_INSTALL_RPATH TRUE
      INSTALL_RPATH "@loader_path/"  # Load dylibs from same directory
  )
endif()

install(TARGETS shoopdaloop
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})

if (OVERRIDE_COMPILE_LOG_LEVEL STREQUAL "")
  if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    message("Building in debug mode. Using compile time log level for back-end: LOG_LEVEL_DEBUG_TRACE")
    target_compile_definitions(shoopdaloop_internals PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_DEBUG_TRACE)
  else()
    message("Building in non-debug mode. Disabling trace messages. Using compile time log level for back-end: LOG_LEVEL_ALWAYS_TRACE")
    target_compile_definitions(shoopdaloop_internals PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_ALWAYS_TRACE)
  endif()
else()
  message(WARNING "Using overridden compile time log level for back-end: ${OVERRIDE_COMPILE_LOG_LEVEL}")
  target_compile_definitions(shoopdaloop_internals PUBLIC -DCOMPILE_LOG_LEVEL=${OVERRIDE_COMPILE_LOG_LEVEL})
endif()

# Generate libshoopdaloop Python bindings into source dir
set(GENERATED_BINDINGS_PY ${CMAKE_CURRENT_BINARY_DIR}/libshoopdaloop_bindings.py)
add_custom_command(
  OUTPUT ${GENERATED_BINDINGS_PY}
  COMMAND ${RUN_CMD_PREFIX} ${CMAKE_COMMAND} -E env ASAN_OPTIONS=detect_leaks=0 ${CTYPESGEN_COMMAND} --no-macro-warnings -lshoopdaloop ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop.h ${CMAKE_CURRENT_SOURCE_DIR}/types.h -o ${GENERATED_BINDINGS_PY}
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop.h ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop.cpp shoopdaloop
  COMMENT "Generating C bindings for libshoopdaloop."
  )
add_custom_target(
  c_bindings
  ALL
  DEPENDS ${GENERATED_BINDINGS_PY}
  )

# Generate javascript copy of enums into source dir
set(PATH_SEP ":")
if (WIN32)
  set(PATH_SEP $<SEMICOLON>)
endif()

install(FILES ${GENERATED_BINDINGS_PY}
    EXCLUDE_FROM_ALL
    COMPONENT package_install
    DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
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
      string(FIND ${ASAN_DSO_CONTENT} "GNU ld script" FOUND_LD_SCRIPT)
      if(NOT ${FOUND_LD_SCRIPT} EQUAL -1)
        # Pointed to a linker script, find the actual lib
        string(REGEX MATCH ".*INPUT[ ]*\\([ ]*([^\)^ ]+)[ ]*\\).*" FOUND_ASAN_SO ${ASAN_DSO_CONTENT})
        set(ASAN_DSO ${CMAKE_MATCH_1})
        message("Found that libasan.so is a linker script pointing to ${ASAN_DSO}.")
      endif()
    endif()
    set(RUN_CMD_PREFIX LD_PRELOAD=${ASAN_DSO} ${RUN_CMD_PREFIX})
    set(RUN_CMD_PREFIX ${RUN_CMD_PREFIX} PARENT_SCOPE)
endif()
option(ENABLE_PROCESS_PROFILING "Enable internal audio processing profiling" ON)
if(ENABLE_PROCESS_PROFILING)
    add_compile_options(-DSHOOP_PROFILING)
endif()
set(OVERRIDE_COMPILE_LOG_LEVEL "" CACHE STRING "If set, overrides the compile-time log level filter. Valid options are LOG_LEVEL_[TRACE, DEBUG, INFO, WARNING, ERROR].")

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

if (HAVE_LV2)
  include (${CMAKE_CURRENT_SOURCE_DIR}/BuildLV2Projects.cmake)
  add_subdirectory(${CMAKE_SOURCE_DIR}/../third_party/lv2_evbuf lv2_evbuf)
  set_target_properties(lv2_evbuf PROPERTIES POSITION_INDEPENDENT_CODE ON)
  add_dependencies(lv2_evbuf lv2_proj)
endif()

# Includes
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal)
include_directories(${CMAKE_CURRENT_SOURCE_DIR})
include_directories(${CMAKE_SOURCE_DIR}/../third_party/nlohmann_json)

add_subdirectory(test)
add_subdirectory(accelerate)

# Backend lib
file(GLOB INTERNAL_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/*.cpp)
if (HAVE_LV2)
  file(GLOB LV2_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/lv2/*.cpp)
  list(APPEND INTERNAL_SOURCES ${LV2_SOURCES})
  message(${INTERNAL_SOURCES})
  include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal/lv2)
endif()
if (BACKEND_JACK)
  file(GLOB JACK_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/jack/*.cpp)
  list(APPEND INTERNAL_SOURCES ${JACK_SOURCES})
  include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal/jack)
endif()

add_library(shoopdaloop_internals STATIC libshoopdaloop.cpp ${INTERNAL_SOURCES})
set_target_properties(shoopdaloop_internals PROPERTIES POSITION_INDEPENDENT_CODE ON)
target_link_options(shoopdaloop_internals PRIVATE -static)
target_include_directories(shoopdaloop_internals
                        PRIVATE
                        ${CMAKE_SOURCE_DIR}/../third_party/base64/include
                        ${CMAKE_CURRENT_BINARY_DIR}
                        ${Boost_INCLUDE_DIR}
                        )
target_link_libraries(shoopdaloop_internals PRIVATE Threads::Threads fmt::fmt)
if (UNIX)
    target_link_libraries(shoopdaloop_internals PUBLIC dl)
endif()
if (HAVE_LV2)
  target_link_libraries(shoopdaloop_internals PRIVATE
                        lv2_evbuf
                        ${LV2_LIBRARIES}
                        ${DEPEND_ON_LILV})
  target_include_directories(shoopdaloop_internals
                          PRIVATE
                          ${CMAKE_SOURCE_DIR}/../third_party/lv2_evbuf
                          ${LV2_INCLUDE_DIRS}
                          ${LILV_INCLUDE_DIRS}
                          ${CMAKE_SOURCE_DIR}/../third_party/lv2_external_ui
                          )
endif()
if (BACKEND_JACK)
  target_include_directories(shoopdaloop_internals PRIVATE ${JACK_INCLUDE_DIRS})
endif()

file(WRITE ${CMAKE_CURRENT_BINARY_DIR}/empty.cpp "")
add_library(shoopdaloop SHARED ${CMAKE_CURRENT_BINARY_DIR}/empty.cpp)
target_link_libraries(shoopdaloop PRIVATE shoopdaloop_internals)
set_property(TARGET shoopdaloop PROPERTY POSITION_INDEPENDENT_CODE ON)

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
    message("Building in debug mode. Using compile time log level: LOG_LEVEL_TRACE")
    target_compile_definitions(shoopdaloop_internals PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_TRACE)
  else()
    message("Building in non-debug mode. Disabling trace messages. Using compile time log level: LOG_LEVEL_DEBUG")
    target_compile_definitions(shoopdaloop_internals PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_DEBUG)
  endif()
else()
  message(WARNING "Using overridden compile time log level: ${OVERRIDE_COMPILE_LOG_LEVEL}")
  target_compile_definitions(shoopdaloop_internals PUBLIC -DCOMPILE_LOG_LEVEL=${OVERRIDE_COMPILE_LOG_LEVEL})
endif()


# Generate libshoopdaloop Python bindings into source dir
set(GENERATED_BINDINGS_PY ${CMAKE_CURRENT_BINARY_DIR}/libshoopdaloop_bindings.py)
add_custom_command(
  OUTPUT ${GENERATED_BINDINGS_PY}
  COMMAND ${RUN_CMD_PREFIX} ${CMAKE_COMMAND} -E env ASAN_OPTIONS=detect_leaks=0 ${CTYPESGEN_COMMAND} --no-macro-warnings -lshoopdaloop ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop.h ${CMAKE_CURRENT_SOURCE_DIR}/types.h -o ${GENERATED_BINDINGS_PY}
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop.h ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop.cpp
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
set(OUR_PYTHONPATH "$ENV{PYTHONPATH}${PATH_SEP}${CMAKE_CURRENT_BINARY_DIR}${PATH_SEP}${PY_BUILD_CMAKE_MODULE_NAME}/..${PATH_SEP}${CMAKE_SOURCE_DIR}/shoopdaloop")
set(GENERATED_TYPES_JS ${CMAKE_CURRENT_BINARY_DIR}/types.js)
add_custom_command(
  OUTPUT ${GENERATED_TYPES_JS}
  COMMAND ${RUN_CMD_PREFIX} ${CMAKE_COMMAND} -E env ASAN_OPTIONS=detect_leaks=0 PYTHONPATH="${OUR_PYTHONPATH}" "${PYTHON_CMD}" ${CMAKE_SOURCE_DIR}/codegen/gen_js_enums.py "${GENERATED_TYPES_JS}"
  COMMENT "Generating Javascript types."
  DEPENDS c_bindings shoopdaloop
  WORKING_DIRECTORY ${CMAKE_BINARY_DIR}/libshoopdaloop
  )
add_custom_target(
  javascript_types
  ALL
  DEPENDS ${GENERATED_TYPES_JS}
  )

install(FILES ${GENERATED_BINDINGS_PY}
    EXCLUDE_FROM_ALL
    COMPONENT package_install
    DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
install(FILES ${GENERATED_TYPES_JS}
    EXCLUDE_FROM_ALL
    COMPONENT package_install
    DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/lib/generated)
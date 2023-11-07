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

# "Build" spdlog (just a single-header)
set(SPDLOG_INCLUDE_DIR ${CMAKE_SOURCE_DIR}/../third_party/spdlog/include)

if (BACKEND_JACK)
  # Generate a wrapper for libjack which allows dynamic loading using dlopen
  find_package(Jack REQUIRED)
  get_filename_component(JACK_LIBNAME ${JACK_LIBRARIES} NAME)
  add_custom_command(
    OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.c ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.h
    COMMAND mkdir -p ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers
    COMMAND ${PYTHON_CMD} "${CMAKE_SOURCE_DIR}/../third_party/dynload-wrapper/generate-wrapper.py" --include "${JACK_INCLUDE_DIRS}/jack/jack.h" --include "${JACK_INCLUDE_DIRS}/jack/midiport.h" --sys-include '<jack/jack.h>' --sys-include '<jack/midiport.h>' --soname "${JACK_LIBNAME}" --init-name jack_wrappers --output-header "${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.h" --output-implementation "${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.c"
    COMMENT "Generating libjack dynload wrapper"
  )
  add_custom_target(
    jack_wrappers_impl
    ALL
    DEPENDS ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.c ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.h
    )
  set(JACKWRAPPERS_INCLUDE_DIRS ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers})
  add_library(jack_wrappers STATIC ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.c)
  set_property(TARGET jack_wrappers PROPERTY POSITION_INDEPENDENT_CODE ON)
  target_include_directories(jack_wrappers PUBLIC ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers)
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

set(STATIC_DEPS_PREFIX ${CMAKE_CURRENT_BINARY_DIR}/static-deps)

if(HAVE_LV2)
  include (${CMAKE_CURRENT_SOURCE_DIR}/BuildStaticDependencies.cmake)
  add_subdirectory(${CMAKE_SOURCE_DIR}/../third_party/lv2_evbuf lv2_evbuf)
  target_compile_options(lv2_evbuf PUBLIC -fPIC)
  add_dependencies(lv2_evbuf lv2)
endif()

# Includes
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal)
include_directories(${CMAKE_CURRENT_SOURCE_DIR})
include_directories(${CMAKE_SOURCE_DIR}/../third_party/nlohmann_json)

add_subdirectory(test)
add_subdirectory(custom_qt_msg_handler)

if (BUILD_QML_EXTENSIONS)
  add_subdirectory(qml_types)
endif()

# Backend lib
file(GLOB INTERNAL_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/internal/*.cpp)
if (HAVE_LV2)
  file(GLOB LV2_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/lv2/*.cpp)
  set(INTERNAL_SOURCES ${INTERNAL_SOURCES} ${LV2_SOURCES})
  include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal/lv2)
endif()
if (BACKEND_JACK)
  file(GLOB JACK_SOURCES ${CMAKE_CURRENT_SOURCE_DIR}/jack/*.cpp)
  set(INTERNAL_SOURCES ${INTERNAL_SOURCES} ${JACK_SOURCES})
  include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal/jack)
endif()

# TODO jack

add_library(shoopdaloop SHARED libshoopdaloop.cpp ${INTERNAL_SOURCES})
target_link_libraries(shoopdaloop PUBLIC
                      ${fmt_LIBRARY})
target_include_directories(shoopdaloop
                        PRIVATE
                        ${CMAKE_SOURCE_DIR}/../third_party/base64/include
                        ${CMAKE_CURRENT_BINARY_DIR}
                        ${Boost_INCLUDE_DIR}
                        ${fmt_INCLUDE_DIR}
                        ${SPDLOG_INCLUDE_DIR}
                        )
if (HAVE_LV2)
  add_dependencies(shoopdaloop ${DEPEND_ON_LILV})
  target_link_libraries(shoopdaloop PUBLIC
                        lv2_evbuf
                        ${LV2_LIBRARIES}
                        ${LILV_LIBRARIES}
                        dl)
  target_include_directories(shoopdaloop
                          PRIVATE
                          ${CMAKE_SOURCE_DIR}/../third_party/lv2_evbuf
                          ${LV2_INCLUDE_DIRS}
                          ${LILV_INCLUDE_DIRS}
                          ${CMAKE_SOURCE_DIR}/../third_party/lv2_external_ui
                          )
endif()
if (BACKEND_JACK)
  add_dependencies(shoopdaloop jack_wrappers)
  target_link_libraries(shoopdaloop PUBLIC jack_wrappers)
  target_include_directories(shoopdaloop PRIVATE ${JACKWRAPPERS_INCLUDE_DIRS})
endif ()
install(TARGETS shoopdaloop
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})

if (OVERRIDE_COMPILE_LOG_LEVEL STREQUAL "")
  if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    message("Building in debug mode. Using compile time log level: LOG_LEVEL_TRACE")
    target_compile_definitions(shoopdaloop PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_TRACE)
  else()
    message("Building in non-debug mode. Disabling trace and debug messages. Using compile time log level: LOG_LEVEL_INFO")
    target_compile_definitions(shoopdaloop PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_INFO)
  endif()
else()
  message(WARNING "Using overridden compile time log level: ${OVERRIDE_COMPILE_LOG_LEVEL}")
  target_compile_definitions(shoopdaloop PUBLIC -DCOMPILE_LOG_LEVEL=${OVERRIDE_COMPILE_LOG_LEVEL})
endif()

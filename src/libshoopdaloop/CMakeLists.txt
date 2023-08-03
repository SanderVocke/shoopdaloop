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
option(ENABLE_PROCESS_PROFILING "Enable internal audio processing profiling")
if(ENABLE_PROCESS_PROFILING)
    add_compile_options(-DSHOOP_PROFILING)
endif()
set(OVERRIDE_COMPILE_LOG_LEVEL "" CACHE STRING "If set, overrides the compile-time log level filter. Valid options are LOG_LEVEL_[TRACE, DEBUG, INFO, WARNING, ERROR].")
option(STATIC_DEPENDENCIES "Use static linkage for non-Python dependencies.")

# Find pkgs
find_package(Boost COMPONENTS ALL REQUIRED)

# Generate a wrapper for libjack which allows dynamic loading using dlopen
find_package(Jack REQUIRED)
add_custom_command(
  OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.c ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.h
  COMMAND mkdir -p ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers && ${CMAKE_SOURCE_DIR}/../third_party/dynload-wrapper/generate-wrapper.py
    --include ${JACK_INCLUDE_DIRS}/jack/jack.h
    --include ${JACK_INCLUDE_DIRS}/jack/midiport.h
    --sys-include '<jack/jack.h>'
    --sys-include '<jack/midiport.h>'
    --soname ${JACK_LIBRARIES}
    --init-name jack_wrappers
    --output-header ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.h
    --output-implementation ${CMAKE_CURRENT_BINARY_DIR}/jack_wrappers/jack_wrappers.c
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


if(STATIC_DEPENDENCIES)

  include (${CMAKE_CURRENT_SOURCE_DIR}/BuildStaticDependencies.cmake)

else()
  set(DEPEND_ON_LILV)

  # Find dynamic versions
  find_package(spdlog REQUIRED)
  find_package(fmt REQUIRED)
  find_package(Lilv REQUIRED)

endif()

# Includes
include_directories(${CMAKE_CURRENT_SOURCE_DIR}/internal)
include_directories(${CMAKE_CURRENT_SOURCE_DIR})

# Subdirs
add_subdirectory(${CMAKE_SOURCE_DIR}/../third_party/lv2_evbuf lv2_evbuf)
add_subdirectory(test)
add_subdirectory(qml_types)
add_subdirectory(custom_qt_msg_handler)

# Backend lib
add_library(shoopdaloop SHARED libshoopdaloop.cpp internal/LoggingBackend.cpp internal/ProcessProfiling.cpp)
add_dependencies(shoopdaloop jack_wrappers ${DEPEND_ON_LILV})
target_link_libraries(shoopdaloop PUBLIC
                      lv2_evbuf
                      ${BOOST_LIBRARIES}
                      jack_wrappers
                      ${LV2_LIBRARIES}
                      ${LILV_LIBRARIES}
                      ${fmt_LIBRARY})
target_include_directories(shoopdaloop
                        PRIVATE
                        ${CMAKE_SOURCE_DIR}/../third_party/lv2_evbuf
                        ${CMAKE_SOURCE_DIR}/../third_party/base64/include
                        ${CMAKE_CURRENT_BINARY_DIR}
                        ${BOOST_INCLUDE_DIRS}
                        ${LV2_INCLUDE_DIRS}
                        ${LILV_INCLUDE_DIRS}
                        ${JACKWRAPPERS_INCLUDE_DIRS}
                        ${fmt_INCLUDE_DIR}
                        ${SPDLOG_INCLUDE_DIR}
                        ${CMAKE_SOURCE_DIR}/../third_party/lv2_external_ui
                        )
install(TARGETS shoopdaloop
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})

if (OVERRIDE_COMPILE_LOG_LEVEL STREQUAL "")
  if (CMAKE_BUILD_TYPE STREQUAL "Debug")
    message("Building in debug mode. Using compile time log level: LOG_LEVEL_TRACE")
    target_compile_definitions(shoopdaloop PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_TRACE)
  else()
    message("Building in non-debug mode. Using compile time log level: LOG_LEVEL_INFO")
    target_compile_definitions(shoopdaloop PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_INFO)
  endif()
else()
  message(WARNING "Using overridden compile time log level: ${OVERRIDE_COMPILE_LOG_LEVEL}")
  target_compile_definitions(shoopdaloop PUBLIC -DCOMPILE_LOG_LEVEL=${OVERRIDE_COMPILE_LOG_LEVEL})
endif()

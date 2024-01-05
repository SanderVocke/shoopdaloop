# Build Breakpad
include (${CMAKE_CURRENT_SOURCE_DIR}/BuildBreakpad.cmake)

add_library(shoop_crashhandling SHARED shoop_crashhandling.cpp)
set_property(TARGET shoop_crashhandling PROPERTY POSITION_INDEPENDENT_CODE ON)
target_include_directories(shoop_crashhandling INTERFACE ${CMAKE_CURRENT_SOURCE_DIR})
target_include_directories(shoop_crashhandling PRIVATE ${BREAKPAD_CLIENT_INCLUDE_DIR} ${BREAKPAD_TOPLEVEL_INCLUDE_DIR} ${BREAKPAD_BUILD_DIR})
target_link_libraries(shoop_crashhandling PRIVATE ${BREAKPAD_CLIENT_LIB} ${BREAKPAD_COMMON_LIB})
add_dependencies(shoop_crashhandling breakpad_proj)
create_breakpad_symbols_dump(shoop_crashhandling)

if(APPLE)
  target_link_libraries(shoop_crashhandling PRIVATE
  "-framework Foundation"
  )
endif()

# Generate C bindings
add_custom_command(
  OUTPUT shoop_crashhandling.py
  COMMAND ${RUN_CMD_PREFIX} ${CMAKE_COMMAND} -E env ASAN_OPTIONS=detect_leaks=0 ${CTYPESGEN_COMMAND} --no-macro-warnings -lshoop_crashhandling ${CMAKE_CURRENT_SOURCE_DIR}/shoop_crashhandling.h -o shoop_crashhandling.py
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/shoop_crashhandling.h
  COMMENT "Generating C bindings for crash handling library."
  )
add_custom_target(
  shoop_crashhandling_bindings
  ALL
  DEPENDS shoop_crashhandling.py
  )

install(TARGETS shoop_crashhandling
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/shoop_crashhandling.py
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
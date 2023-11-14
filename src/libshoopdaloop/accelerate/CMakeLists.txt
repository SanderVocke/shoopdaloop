add_library(shoop_accelerate SHARED shoop_accelerate.cpp)
set_property(TARGET shoop_accelerate PROPERTY POSITION_INDEPENDENT_CODE ON)
if(UNIX)
  target_link_options(shoop_accelerate PRIVATE -static-libstdc++)
endif()
target_include_directories(shoop_accelerate INTERFACE ${CMAKE_CURRENT_SOURCE_DIR})

# Generate C bindings
add_custom_command(
  OUTPUT shoop_accelerate.py
  COMMAND ${RUN_CMD_PREFIX} ${CMAKE_COMMAND} -E env ASAN_OPTIONS=detect_leaks=0 ${CTYPESGEN_COMMAND} --no-macro-warnings -lshoop_accelerate ${CMAKE_CURRENT_SOURCE_DIR}/shoop_accelerate.h -o shoop_accelerate.py
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/shoop_accelerate.h
  COMMENT "Generating C bindings for acceleration extensions."
  )
add_custom_target(
  shoop_accelerate_bindings
  ALL
  DEPENDS shoop_accelerate.py
  )

install(TARGETS shoop_accelerate
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/shoop_accelerate.py
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})

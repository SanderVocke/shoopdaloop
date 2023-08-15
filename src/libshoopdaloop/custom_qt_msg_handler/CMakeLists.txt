add_library(custom_qt_msg_handler SHARED custom_qt_msg_handler.cpp)
set_property(TARGET custom_qt_msg_handler PROPERTY POSITION_INDEPENDENT_CODE ON)
target_link_libraries(custom_qt_msg_handler PRIVATE Qt6::Core ${fmt_LIBRARY} shoopdaloop)
target_include_directories(custom_qt_msg_handler PRIVATE ${fmt_INCLUDE_DIR} ${SPDLOG_INCLUDE_DIR})

# Generate C bindings
add_custom_command(
  OUTPUT custom_qt_msg_handler.py
  COMMAND ${RUN_CMD_PREFIX} ASAN_OPTIONS=detect_leaks=0 ${CTYPESGEN_COMMAND} -lcustom_qt_msg_handler ${CMAKE_CURRENT_SOURCE_DIR}/custom_qt_msg_handler.h -o custom_qt_msg_handler.py
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/custom_qt_msg_handler.h
  COMMENT "Generating C bindings for custom Qt message handler."
  )
add_custom_target(
  custom_qt_msg_handler_c_bindings
  ALL
  DEPENDS custom_qt_msg_handler.py
  )
install(TARGETS custom_qt_msg_handler
  EXCLUDE_FROM_ALL
  COMPONENT package_install
  DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/custom_qt_msg_handler.py
  EXCLUDE_FROM_ALL
  COMPONENT package_install
  DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
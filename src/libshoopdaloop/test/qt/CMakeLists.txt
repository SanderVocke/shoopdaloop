add_library(qml_tests SHARED qml_tests.cpp)
target_link_libraries(qml_tests PRIVATE Qt6::QuickTest)

# Generate C bindings
add_custom_command(
  OUTPUT qml_tests.py
  COMMAND ${RUN_CMD_PREFIX} ${CMAKE_COMMAND} -E env ASAN_OPTIONS=detect_leaks=0 ${CTYPESGEN_COMMAND} -lqml_tests ${CMAKE_CURRENT_SOURCE_DIR}/qml_tests.h -o qml_tests.py
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/qml_tests.h
  COMMENT "Generating C bindings for QML test interface."
  )
add_custom_target(
  qml_test_c_bindings
  ALL
  DEPENDS qml_tests.py
  )
install(TARGETS qml_tests
  EXCLUDE_FROM_ALL
  COMPONENT package_install
  DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
install(FILES $<TARGET_PDB_FILE:qml_tests> DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME} OPTIONAL)
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/qml_tests.py
  EXCLUDE_FROM_ALL
  COMPONENT package_install
  DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
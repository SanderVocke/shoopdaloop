add_library(qml_tests SHARED qml_tests.cpp)
target_link_libraries(qml_tests PRIVATE Qt6::QuickTest)

# Generate C bindings
add_custom_command(
  OUTPUT qml_tests.py
  COMMAND ctypesgen -lqml_tests ${CMAKE_CURRENT_SOURCE_DIR}/qml_tests.h -o qml_tests.py
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/qml_tests.h
  COMMENT "Generating C bindings for QML test interface."
  )
add_custom_target(
  qml_test_c_bindings
  ALL
  DEPENDS qml_tests.py
  )
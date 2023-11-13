add_library(qt_extensions SHARED render_audio_waveform.cpp)
target_link_libraries(qt_extensions PRIVATE Qt6::Quick)

set(QT_EXTENSIONS_INTERFACE_HEADERS
    ${CMAKE_CURRENT_SOURCE_DIR}/qt_extensions.h
    ${CMAKE_CURRENT_SOURCE_DIR}/render_audio_waveform.h)

# Generate C bindings
add_custom_command(
  OUTPUT qt_extensions.py
  COMMAND ${RUN_CMD_PREFIX} ${CMAKE_COMMAND} -E env ASAN_OPTIONS=detect_leaks=0 ${CTYPESGEN_COMMAND} -lqt_extensions ${QT_EXTENSIONS_INTERFACE_HEADERS} -o qt_extensions.py
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/qt_extensions.h
  COMMENT "Generating C bindings for Qt extensions."
  )
add_custom_target(
  qt_extensions_bindings
  ALL
  DEPENDS qt_extensions.py
  )

install(TARGETS qt_extensions
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
install(FILES ${CMAKE_CURRENT_BINARY_DIR}/qt_extensions.py
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})

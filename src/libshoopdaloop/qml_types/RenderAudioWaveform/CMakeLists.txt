# Also register the QML type directly, so that it can be used in QML without any
# Shiboken in-between.
# PLUGIN_TARGET ensures that the result is a single .so file (not multiple dependent ones).
# This is important because when installed into the QML plug-in directory, the Linux dynamic
# linker won't be able to find any other .so files in the Python site-packages.

set(QMLTYPES_FILE ${CMAKE_CURRENT_BINARY_DIR}/RenderAudioWaveform.qmltypes)
set(QMLDIR_FILE ${CMAKE_CURRENT_BINARY_DIR}/qmldir)
file(READ ${CMAKE_CURRENT_SOURCE_DIR}/RenderAudioWaveform.qmltypes QMLTYPES_FILE_CONTENTS)
file(READ ${CMAKE_CURRENT_SOURCE_DIR}/qmldir QMLDIR_FILE_CONTENTS)

if(WIN32)
    # Hacky, but for some reason cannot get the qmltypes auto-generation to work on Windows.
    # Therefore, we pass the info manually on Windows, and on Linux we add a new rule to check
    # that the auto-generated output still matches our "hand-written" one.
    qt_add_qml_module(RenderAudioWaveform
        PLUGIN_TARGET RenderAudioWaveform
        URI RenderAudioWaveform
        VERSION 1.0
        SOURCES
            RenderAudioWaveform.cpp RenderAudioWaveform.h
        DEPENDENCIES QtQuick
        NO_GENERATE_QMLTYPES
        NO_GENERATE_QMLDIR
    )
    file(WRITE ${QMLTYPES_FILE} ${QMLTYPES_FILE_CONTENTS})
    file(WRITE ${QMLDIR_FILE} ${QMLDIR_FILE_CONTENTS})
else()
    qt_add_qml_module(RenderAudioWaveform
        PLUGIN_TARGET RenderAudioWaveform
        URI RenderAudioWaveform
        VERSION 1.0
        SOURCES
            RenderAudioWaveform.cpp RenderAudioWaveform.h
        DEPENDENCIES QtQuick
    )
    file(READ ${QMLTYPES_FILE} QMLTYPES_FILE_GENERATED)
    file(READ ${QMLDIR_FILE} QMLDIR_FILE_GENERATED)
    string(STRIP "${QMLTYPES_FILE_GENERATED}" QMLTYPES_FILE_GENERATED)
    string(STRIP "${QMLDIR_FILE_GENERATED}" QMLDIR_FILE_GENERATED)
    string(STRIP "${QMLTYPES_FILE_CONTENTS}" QMLTYPES_FILE_CONTENTS)
    string(STRIP "${QMLDIR_FILE_CONTENTS}" QMLDIR_FILE_CONTENTS)
    if(NOT "${QMLTYPES_FILE_GENERATED}" STREQUAL "${QMLTYPES_FILE_CONTENTS}")
        message(FATAL_ERROR "qmltypes file does not match the hard-coded one which is used for Windows.")
    endif()
    if(NOT "${QMLDIR_FILE_GENERATED}" STREQUAL "${QMLDIR_FILE_CONTENTS}")
        message(FATAL_ERROR "qmldir file does not match the hard-coded one which is used for Windows.")
    endif()
endif()

target_link_libraries(RenderAudioWaveform PRIVATE Qt6::Quick)

# Install the type
install(FILES ${QMLDIR_FILE} ${QMLTYPES_FILE}
        DESTINATION
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/qml_types/RenderAudioWaveform/)
install(FILES $<TARGET_FILE:RenderAudioWaveform>
        DESTINATION
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/qml_plugins/)
# Also register the QML type directly, so that it can be used in QML without any
# Shiboken in-between.
# PLUGIN_TARGET ensures that the result is a single .so file (not multiple dependent ones).
# This is important because when installed into the QML plug-in directory, the Linux dynamic
# linker won't be able to find any other .so files in the Python site-packages.
qt_add_qml_module(RenderAudioWaveform
    PLUGIN_TARGET RenderAudioWaveform
    URI RenderAudioWaveform
    VERSION 1.0
    SOURCES
        RenderAudioWaveform.cpp RenderAudioWaveform.h
    DEPENDENCIES QtQuick
    SHARED
)

target_link_libraries(RenderAudioWaveform PRIVATE Qt6::Quick)
target_compile_definitions(RenderAudioWaveform PRIVATE -DRENDERAUDIOWAVEFORM_LIBRARY)

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
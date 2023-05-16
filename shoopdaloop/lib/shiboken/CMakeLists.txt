find_package(Qt6 REQUIRED COMPONENTS Core Gui Quick)
qt_standard_project_setup()

set(shiboken_sources
    RenderAudioWaveform.cpp
    )

add_library(shoopdaloop_shiboken
    ${shiboken_sources}
)
target_link_libraries(shoopdaloop_shiboken PRIVATE Qt6::Core Qt6::Gui Qt6::Quick)
set_property(TARGET shoopdaloop_shiboken PROPERTY PREFIX "")
target_compile_definitions(shoopdaloop_shiboken PRIVATE BINDINGS_BUILD)

# Generate bindings
set(generated_sources
    ${CMAKE_CURRENT_BINARY_DIR}/shoopdaloop_shiboken/shoopdaloop_shiboken_module_wrapper.cpp
    ${CMAKE_CURRENT_BINARY_DIR}/shoopdaloop_shiboken/renderaudiowaveform_wrapper.cpp)

set(generated_sources_dependencies shoopdaloop_shiboken.h shoopdaloop_shiboken.xml)

add_custom_command(OUTPUT ${generated_sources}
    COMMAND shiboken6
        --generator-set=shiboken --enable-parent-ctor-heuristic
        --enable-return-value-heuristic --use-isnull-as-nb_nonzero
        --avoid-protected-hack
        -I${CMAKE_CURRENT_SOURCE_DIR}
        -I/usr/include/qt6
        -T${CMAKE_CURRENT_SOURCE_DIR}
        -T/usr/share/PySide6/typesystems
        --output-directory=${CMAKE_CURRENT_BINARY_DIR}
        ${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop_shiboken.h
        ${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop_shiboken.xml
    DEPENDS ${generated_sources_dependencies}
    IMPLICIT_DEPENDS CXX shoopdaloop_shiboken.h
    WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
    COMMENT "Running generator for shoopdaloop_shiboken.xml.")

add_custom_target(
    shoopdaloop_shiboken_bindings
    ALL
    DEPENDS ${generated_sources}
)
find_program(QSB_EXEC qsb PATHS /usr PATH_SUFFIXES lib/qt6/bin REQUIRED)

file(GLOB SHADERS *.vertex *.fragment)

foreach(SHADER ${SHADERS})
    get_filename_component(TARGET ${SHADER} NAME)
    set(OUTFILE ${CMAKE_CURRENT_BINARY_DIR}/${TARGET}.qsb)
    add_custom_command(
        OUTPUT ${OUTFILE}
        COMMAND ${QSB_EXEC} ${SHADER} -o ${OUTFILE} --glsl "140,330"
        DEPENDS ${SHADER}
        COMMENT "Generating shader: ${SHADER}."
    )
    add_custom_target(
        ${TARGET}
        ALL
        DEPENDS ${OUTFILE}
    )
endforeach()
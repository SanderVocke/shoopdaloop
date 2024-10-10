# use this function on any target you want to generate symbols for.
function(create_breakpad_symbols_dump TARGET)
  add_custom_command(TARGET ${TARGET} POST_BUILD
    COMMAND ${PYTHON_CMD} ${CMAKE_SOURCE_DIR}/../../scripts/create_breakpad_symbols.py ${BREAKPAD_DUMP_SYMS} $<TARGET_FILE:${TARGET}> ${BREAKPAD_SYMBOLS_DIR}
    WORKING_DIRECTORY ${BREAKPAD_SYMBOLS_DIR}
    COMMENT "Dumping breakpad symbols for ${TARGET}"
  )
endfunction()

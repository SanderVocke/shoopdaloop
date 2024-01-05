include(ExternalProject)

set(BREAKPAD_SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/breakpad)
set(BREAKPAD_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/breakpad_build)
set(BREAKPAD_TOPLEVEL_INCLUDE_DIR ${BREAKPAD_SOURCE_DIR}/src)
set(BREAKPAD_BUILD_TARGETS breakpad breakpad_common dump_syms minidump_stackwalk)
if(WIN32)
set(BREAKPAD_CLIENT_LIB ${BREAKPAD_BUILD_DIR}/breakpad.lib)
set(BREAKPAD_COMMON_LIB ${BREAKPAD_BUILD_DIR}/breakpad_common.lib)
set(BREAKPAD_DUMP_SYMS ${BREAKPAD_BUILD_DIR}/dump_syms.exe)
set(BREAKPAD_MINIDUMP_STACKWALK ${BREAKPAD_BUILD_DIR}/minidump_stackwalk.exe)
else()
set(BREAKPAD_CLIENT_LIB ${BREAKPAD_BUILD_DIR}/libbreakpad.a)
set(BREAKPAD_COMMON_LIB ${BREAKPAD_BUILD_DIR}/libbreakpad_common.a)
set(BREAKPAD_DUMP_SYMS ${BREAKPAD_BUILD_DIR}/dump_syms)
set(BREAKPAD_MINIDUMP_STACKWALK ${BREAKPAD_BUILD_DIR}/minidump_stackwalk)
endif()
set(BREAKPAD_OUTPUTS ${BREAKPAD_CLIENT_LIB} ${BREAKPAD_COMMON_LIB} ${BREAKPAD_DUMP_SYMS} ${BREAKPAD_MINIDUMP_STACKWALK})

if (WIN32)
  set(BREAKPAD_CLIENT_INCLUDE_DIR ${BREAKPAD_SOURCE_DIR}/src/client/windows)
elseif (APPLE)
  set(BREAKPAD_CLIENT_INCLUDE_DIR ${BREAKPAD_SOURCE_DIR}/src/client/mac)
else()
  set(BREAKPAD_CLIENT_INCLUDE_DIR ${BREAKPAD_SOURCE_DIR}/src/client/linux)
endif()

set(CMAKE_BUILD_TARGET_ARGS)
foreach(item ${BREAKPAD_BUILD_TARGETS})
    list(APPEND CMAKE_BUILD_TARGET_ARGS --target ${item})
endforeach()

find_package(ZLIB REQUIRED)

set(BKPAD_CMAKE_ARGS 
  -DZLIB_LIBRARY=${ZLIB_LIBRARY}
  -DBKPAD_UNITTESTS=0
  -DPROCESSOR_UNITTESTS=0
  -DBKPAD_DUMP_SYMS=ON
  -DBKPAD_MINIDUMP=ON
  -DCMAKE_POSITION_INDEPENDENT_CODE=ON
  -DCMAKE_CXX_FLAGS="-Wno-c++11-narrowing"
  -DCMAKE_BUILD_TYPE=${CMAKE_BUILD_TYPE})
ExternalProject_Add(breakpad_proj
  CMAKE_ARGS ${BKPAD_CMAKE_ARGS}
  BUILD_COMMAND cmake --build <BINARY_DIR> ${CMAKE_BUILD_TARGET_ARGS}
  INSTALL_COMMAND ""
  SOURCE_DIR ${BREAKPAD_SOURCE_DIR}
  BINARY_DIR ${BREAKPAD_BUILD_DIR}
  BUILD_BYPRODUCTS ${BREAKPAD_OUTPUTS}
)

# All breakpad symbols files to be collected here
set(BREAKPAD_SYMBOLS_DIR "${CMAKE_BINARY_DIR}/breakpad_symbols")
file(MAKE_DIRECTORY ${BREAKPAD_SYMBOLS_DIR})

set(BREAKPAD_SYMBOLS_DIR ${BREAKPAD_SYMBOLS_DIR} PARENT_SCOPE)
set(BREAKPAD_BUILD_DIR ${BREAKPAD_BUILD_DIR} PARENT_SCOPE)
set(BREAKPAD_DUMP_SYMS ${BREAKPAD_DUMP_SYMS} PARENT_SCOPE)

install(FILES ${BREAKPAD_DUMP_SYMS} ${BREAKPAD_MINIDUMP_STACKWALK}
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/crash_handling)
install(DIRECTORY ${BREAKPAD_SYMBOLS_DIR}
      EXCLUDE_FROM_ALL
      COMPONENT package_install
      DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/crash_handling)

# use this function on any target you want to generate symbols for.
function(create_breakpad_symbols_dump TARGET)
  add_custom_command(TARGET ${TARGET} POST_BUILD
    COMMAND ${PYTHON_CMD} ${CMAKE_SOURCE_DIR}/../scripts/create_breakpad_symbols.py ${BREAKPAD_DUMP_SYMS} $<TARGET_FILE:${TARGET}> ${BREAKPAD_SYMBOLS_DIR}
    WORKING_DIRECTORY ${BREAKPAD_SYMBOLS_DIR}
    COMMENT "Dumping breakpad symbols for ${TARGET}"
  )
  add_dependencies(${TARGET} breakpad_proj)
endfunction()

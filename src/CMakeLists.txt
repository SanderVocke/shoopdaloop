cmake_minimum_required(VERSION 3.22)
project(shoopdaloop)

# Set up language settings
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)
set(CMAKE_EXPORT_COMPILE_COMMANDS YES)

# Required for custom QML types
find_package(Qt6 REQUIRED COMPONENTS Core Qml Gui Quick Widgets QuickTest)
qt_standard_project_setup()

set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}" "${CMAKE_SOURCE_DIR}/cmake")

option(ENABLE_COVERAGE "Enable code coverage" OFF)
if(ENABLE_COVERAGE)
  include (CodeCoverage)
  append_coverage_compiler_flags()

  # Install all the files that LCOV will need in order to do back-end
  # code coverage.
  install(DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
          DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/coverage/build
          COMPONENT package_install
          FILES_MATCHING PATTERN "*.gcno" PATTERN "*.o"
  )
  install(DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
          DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/coverage/src
          COMPONENT package_install
          FILES_MATCHING PATTERN "*.cpp" PATTERN "*.c" PATTERN "*.h" PATTERN "*.hpp"
  )
endif()

set(CTYPESGEN_COMMAND ctypesgen)

add_subdirectory(libshoopdaloop)

# Generate libshoopdaloop Python bindings into source dir
set(GENERATED_BINDINGS_PY ${CMAKE_CURRENT_BINARY_DIR}/libshoopdaloop_bindings.py)
add_custom_command(
  OUTPUT ${GENERATED_BINDINGS_PY}
  COMMAND ${RUN_CMD_PREFIX} ASAN_OPTIONS=detect_leaks=0 ${CTYPESGEN_COMMAND} --no-macro-warnings -lshoopdaloop ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop/libshoopdaloop.h ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop/types.h -o ${GENERATED_BINDINGS_PY}
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop/libshoopdaloop.h ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop/libshoopdaloop.cpp
  COMMENT "Generating C bindings for libshoopdaloop."
  )
add_custom_target(
  c_bindings
  ALL
  DEPENDS ${GENERATED_BINDINGS_PY}
  )

# Generate javascript copy of enums into source dir
set(OUR_PYTHONPATH "$ENV{PYTHONPATH}:${CMAKE_CURRENT_BINARY_DIR}:${PY_BUILD_CMAKE_MODULE_NAME}/..")
set(GENERATED_TYPES_JS ${CMAKE_CURRENT_BINARY_DIR}/types.js)
add_custom_command(
  OUTPUT ${GENERATED_TYPES_JS}
  COMMAND ${RUN_CMD_PREFIX} ASAN_OPTIONS=detect_leaks=0 PYTHONPATH=${OUR_PYTHONPATH} LD_LIBRARY_PATH=${CMAKE_CURRENT_BINARY_DIR}/libshoopdaloop python3 gen_js_enums.py ${GENERATED_TYPES_JS}
  COMMENT "Generating Javascript types."
  DEPENDS c_bindings shoopdaloop
  WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/codegen
  )
add_custom_target(
  javascript_types
  ALL
  DEPENDS ${GENERATED_TYPES_JS}
  )

install(FILES ${GENERATED_BINDINGS_PY}
    EXCLUDE_FROM_ALL
    COMPONENT package_install
    DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
install(FILES ${GENERATED_TYPES_JS}
    EXCLUDE_FROM_ALL
    COMPONENT package_install
    DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/lib/generated)
  
# Generate and install version.txt and description.txt
add_custom_command(
  OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/version.txt
  COMMAND bash -c "bash ${CMAKE_SOURCE_DIR}/../scripts/parse_pyproject_toml.sh ${CMAKE_SOURCE_DIR}/../pyproject.toml version >version.txt"
  COMMENT "Generating version.txt"
  WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
)
add_custom_target(version_txt ALL DEPENDS ${CMAKE_CURRENT_BINARY_DIR}/version.txt)
add_custom_command(
  OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/description.txt
  COMMAND bash -c "bash ${CMAKE_SOURCE_DIR}/../scripts/parse_pyproject_toml.sh ${CMAKE_SOURCE_DIR}/../pyproject.toml description >description.txt"
  COMMENT "Generating description.txt"
  WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
)
add_custom_target(description_txt ALL DEPENDS ${CMAKE_CURRENT_BINARY_DIR}/description.txt)

install(FILES ${CMAKE_CURRENT_BINARY_DIR}/version.txt ${CMAKE_CURRENT_BINARY_DIR}/description.txt
    EXCLUDE_FROM_ALL
    COMPONENT package_install
    DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
  

# Include app test scripts
install(FILES ${CMAKE_SOURCE_DIR}/../scripts/test_run_app.sh
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        PERMISSIONS WORLD_EXECUTE OWNER_EXECUTE GROUP_EXECUTE GROUP_WRITE OWNER_WRITE GROUP_READ OWNER_READ WORLD_READ
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/scripts)
install(FILES ${CMAKE_SOURCE_DIR}/../scripts/run_and_generate_coverage.sh
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        PERMISSIONS WORLD_EXECUTE OWNER_EXECUTE GROUP_EXECUTE GROUP_WRITE OWNER_WRITE GROUP_READ OWNER_READ WORLD_READ
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/scripts)

cmake_minimum_required(VERSION 3.22)
project(shoopdaloop)

# Set up language settings
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)
set(CMAKE_EXPORT_COMPILE_COMMANDS YES)

set(PYTHON_CMD python)
set(GLOB_CMD ${PYTHON_CMD} "${CMAKE_SOURCE_DIR}/../scripts/do_glob.py")
set(RUN_WITH_ENV_CMD ${PYTHON_CMD} "${CMAKE_SOURCE_DIR}/../scripts/run_with_env_from_files.py")

# Qt6 is required for custom QML types.
find_package(Qt6 REQUIRED COMPONENTS Core Qml Gui Quick Widgets QuickTest)
qt_standard_project_setup()

set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}" "${CMAKE_SOURCE_DIR}/cmake")

option(ENABLE_COVERAGE "Enable code coverage" OFF)
if(ENABLE_COVERAGE)
  include (CodeCoverage)
  append_coverage_compiler_flags()
endif()

option(BACKEND_JACK "Enable JACK audio backend" ON)
if(BACKEND_JACK)
  add_compile_definitions(-DSHOOP_HAVE_BACKEND_JACK)
endif()

option(HAVE_LV2 "Enable LV2 plugin hosting" ON)
if(HAVE_LV2)
  add_compile_definitions(-DSHOOP_HAVE_LV2)
endif()

set(CTYPESGEN_COMMAND ctypesgen)

add_subdirectory(libshoopdaloop)

# Generate libshoopdaloop Python bindings into source dir
set(GENERATED_BINDINGS_PY ${CMAKE_CURRENT_BINARY_DIR}/libshoopdaloop_bindings.py)
add_custom_command(
  OUTPUT ${GENERATED_BINDINGS_PY}
  COMMAND ${RUN_CMD_PREFIX} ${CMAKE_COMMAND} -E env ASAN_OPTIONS=detect_leaks=0 ${CTYPESGEN_COMMAND} --no-macro-warnings -lshoopdaloop ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop/libshoopdaloop.h ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop/types.h -o ${GENERATED_BINDINGS_PY}
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop/libshoopdaloop.h ${CMAKE_CURRENT_SOURCE_DIR}/libshoopdaloop/libshoopdaloop.cpp
  COMMENT "Generating C bindings for libshoopdaloop."
  )
add_custom_target(
  c_bindings
  ALL
  DEPENDS ${GENERATED_BINDINGS_PY}
  )

# Generate javascript copy of enums into source dir
set(PATH_SEP ":")
if (WIN32)
  set(PATH_SEP $<SEMICOLON>)
endif()
set(OUR_PYTHONPATH "$ENV{PYTHONPATH}${PATH_SEP}${CMAKE_CURRENT_BINARY_DIR}${PATH_SEP}${PY_BUILD_CMAKE_MODULE_NAME}/..${PATH_SEP}${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop")
set(GENERATED_TYPES_JS ${CMAKE_CURRENT_BINARY_DIR}/types.js)
add_custom_command(
  OUTPUT ${GENERATED_TYPES_JS}
  COMMAND ${RUN_CMD_PREFIX} ${CMAKE_COMMAND} -E env ASAN_OPTIONS=detect_leaks=0 PYTHONPATH="${OUR_PYTHONPATH}" LD_LIBRARY_PATH="${CMAKE_CURRENT_BINARY_DIR}/libshoopdaloop" "${PYTHON_CMD}" gen_js_enums.py "${GENERATED_TYPES_JS}"
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
  COMMAND ${PYTHON_CMD} ${CMAKE_SOURCE_DIR}/../scripts/parse_pyproject_toml.py ${CMAKE_SOURCE_DIR}/../pyproject.toml project::version > version.txt
  COMMENT "Generating version.txt"
  WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
)
add_custom_target(version_txt ALL DEPENDS ${CMAKE_CURRENT_BINARY_DIR}/version.txt)
add_custom_command(
  OUTPUT ${CMAKE_CURRENT_BINARY_DIR}/description.txt
  COMMAND ${PYTHON_CMD} ${CMAKE_SOURCE_DIR}/../scripts/parse_pyproject_toml.py ${CMAKE_SOURCE_DIR}/../pyproject.toml project::description > description.txt
  COMMENT "Generating description.txt"
  WORKING_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}
)
add_custom_target(description_txt ALL DEPENDS ${CMAKE_CURRENT_BINARY_DIR}/description.txt)

install(FILES ${CMAKE_CURRENT_BINARY_DIR}/version.txt ${CMAKE_CURRENT_BINARY_DIR}/description.txt
    EXCLUDE_FROM_ALL
    COMPONENT package_install
    DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
  

# Include app test scripts
install(FILES ${CMAKE_SOURCE_DIR}/../scripts/repeat_run_app.py
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        PERMISSIONS WORLD_EXECUTE OWNER_EXECUTE GROUP_EXECUTE GROUP_WRITE OWNER_WRITE GROUP_READ OWNER_READ WORLD_READ
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/scripts)
install(FILES ${CMAKE_SOURCE_DIR}/../scripts/run_and_generate_coverage.sh
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        PERMISSIONS WORLD_EXECUTE OWNER_EXECUTE GROUP_EXECUTE GROUP_WRITE OWNER_WRITE GROUP_READ OWNER_READ WORLD_READ
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME}/scripts)

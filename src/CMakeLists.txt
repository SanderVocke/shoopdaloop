cmake_minimum_required(VERSION 3.25)

set(CMAKE_OSX_ARCHITECTURES "${ARCHS_STANDARD}") # Enables a universal build on MacOS where possible

#  Enable CMAKE_MSVC_DEBUG_INFORMATION_FORMAT to override the cmake pdb flags easily
if (POLICY CMP0141)
  cmake_policy(SET CMP0141 NEW)
endif (POLICY CMP0141)

project(shoopdaloop)

# Set up language settings
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED YES)
set(CMAKE_CXX_EXTENSIONS NO)
set(CMAKE_EXPORT_COMPILE_COMMANDS YES)
set(CMAKE_VISIBILITY_INLINES_HIDDEN 1)

find_package(Python3 COMPONENTS Interpreter REQUIRED)
set(PYTHON_CMD ${Python3_EXECUTABLE})
set(GLOB_CMD ${PYTHON_CMD} "${CMAKE_SOURCE_DIR}/../scripts/do_glob.py")
set(RUN_WITH_ENV_CMD ${PYTHON_CMD} "${CMAKE_SOURCE_DIR}/../scripts/run_with_env_from_files.py")

find_package(Threads REQUIRED)

set(CMAKE_MODULE_PATH ${CMAKE_MODULE_PATH} "${CMAKE_CURRENT_SOURCE_DIR}" "${CMAKE_SOURCE_DIR}/cmake")

# Ensure PDB file generation
if (WIN32)
  set(CMAKE_PDB_OUTPUT_DIRECTORY "${CMAKE_CURRENT_BINARY_DIR}/pdb")
  if (CMAKE_CXX_COMPILER_ID MATCHES "MSVC")
    add_compile_options("/Zi")
    add_link_options("/DEBUG")
    add_link_options("/OPT:REF")
    add_link_options("/OPT:ICF")
  elseif(CMAKE_CXX_COMPILER_ID MATCHES "Clang")
    add_compile_options(-g -gcodeview)
    add_link_options(-fuse-ld=lld -Wl,--pdb=)
  endif()
endif()

option(ENABLE_COVERAGE "Enable code coverage" OFF)
if(ENABLE_COVERAGE)
  include (CodeCoverage)
  append_coverage_compiler_flags()
endif()

option(BACKEND_JACK "Enable JACK audio backend" ON)
if(BACKEND_JACK)
  add_compile_definitions(SHOOP_HAVE_BACKEND_JACK)
endif()

option(HAVE_LV2 "Enable LV2 plugin hosting" ON)
if(HAVE_LV2)
  add_compile_definitions(SHOOP_HAVE_LV2)
endif()

if (APPLE)
  message(STATUS "MacOS architectures included: ${CMAKE_OSX_ARCHITECTURES}")
endif()

set(CTYPESGEN_COMMAND ctypesgen)

set(PREV_SETTING ${CMAKE_POSITION_INDEPENDENT_CODE})
set(CMAKE_POSITION_INDEPENDENT_CODE TRUE)
add_subdirectory(third_party/fmt)
add_subdirectory(third_party/Catch2)
set(CMAKE_POSITION_INDEPENDENT_CODE ${PREV_SETTING})

add_subdirectory(libshoopdaloop)
  
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

set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.6.3 EXACT COMPONENTS Core Gui Qml QuickControls2 QmlImportScanner REQUIRED)
find_package(Corrosion QUIET)

set(QMAKE_PATH ${Qt6_DIR}/../../../bin)
set(QMAKE ${QMAKE_PATH}/qmake)
message(STATUS "Found Qt at ${Qt6_DIR}. QMake: ${QMAKE}")
set_target_properties(Qt::qmake PROPERTIES IMPORTED_LOCATION ${QMAKE})
set_target_properties(Qt6::qmake PROPERTIES IMPORTED_LOCATION ${QMAKE})
list(PREPEND CMAKE_PROGRAM_PATH ${QMAKE_PATH})
if("${CMAKE_HOST_SYSTEM}" MATCHES ".*Windows.*")
    set(PATH_SEPARATOR "\\;")
else()
    set(PATH_SEPARATOR ":")
endif()
set(ENV{PATH} "${QMAKE_PATH}${PATH_SEPARATOR}$ENV{PATH}")

message(STATUS $ENV{PATH})
execute_process(
    COMMAND qmake --version
)

# Print the results to the CMake output
message(STATUS "Result: ${result}")

set(CRATE shoop_rs)
corrosion_import_crate(MANIFEST_PATH Cargo.toml CRATES ${CRATE})

# The Rust library's build script needs to be told where to output the
# generated headers so CMake can find them.
set(CXXQT_EXPORT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cxxqt")
corrosion_set_env_vars(
    ${CRATE}
    "CXXQT_EXPORT_DIR=${CXXQT_EXPORT_DIR}"
    "QMAKE=${QMAKE}"
)

add_library(shoop_rs_lib INTERFACE)
target_include_directories(shoop_rs_lib INTERFACE "${CXXQT_EXPORT_DIR}/${CRATE}")
target_link_libraries(
    shoop_rs_lib
    INTERFACE
    # WHOLE_ARCHIVE is needed for the generated QML plugin to register on startup,
    # otherwise the linker will discard the static variables that initialize it.
    "$<LINK_LIBRARY:WHOLE_ARCHIVE,${CRATE}-static>"
    Qt::Core
    Qt::Gui
    Qt::Qml
    Qt::QuickControls2
)

# The final shared lib links the static lib and type registration call.
add_library(shoop_rs_dynlib SHARED shoop_rs.cpp)
target_link_libraries(shoop_rs_dynlib PRIVATE shoop_rs_lib)
target_include_directories(shoop_rs_dynlib PRIVATE "${CMAKE_CURRENT_SOURCE_DIR}/src/cxx")

install(TARGETS shoop_rs_dynlib
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
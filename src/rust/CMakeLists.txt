set(CMAKE_AUTOMOC ON)

find_package(Qt6 6.6.3 EXACT COMPONENTS Core Gui Qml QuickControls2 QmlImportScanner Test REQUIRED)
find_package(Corrosion QUIET)
include_directories(${Qt6_INCLUDE_DIRS})

set(QMAKE_PATH ${Qt6_DIR}/../../../bin)
set(QMAKE ${QMAKE_PATH}/qmake)
message(STATUS "Found Qt at ${Qt6_DIR}. QMake: ${QMAKE}. Include dirs: ${Qt6_INCLUDE_DIRS}")
set_target_properties(Qt::qmake PROPERTIES IMPORTED_LOCATION ${QMAKE})
set_target_properties(Qt6::qmake PROPERTIES IMPORTED_LOCATION ${QMAKE})
list(PREPEND CMAKE_PROGRAM_PATH ${QMAKE_PATH})
if("${CMAKE_HOST_SYSTEM}" MATCHES ".*Windows.*")
    set(PATH_SEPARATOR "\\;")
else()
    set(PATH_SEPARATOR ":")
endif()
set(ENV{PATH} "${QMAKE_PATH}${PATH_SEPARATOR}$ENV{PATH}")

execute_process(
    COMMAND qmake --version
)

set(ENV{LIBSHOOPDALOOP_DIR} "${CMAKE_BINARY_DIR}/libshoopdaloop")

add_subdirectory(shoop_rs_frontend)
# Subdirs
add_subdirectory(unit)
add_subdirectory(integration)
add_subdirectory(tools)

# Test runner
add_executable(test_runner
    test_runner.cpp
    unit/test_AudioLoop.cpp
    unit/test_ObjectPool.cpp
    unit/test_BasicLoop.cpp
    integration/test_synced_BasicLoops.cpp
    )
target_link_libraries(test_runner PRIVATE
                      ${BOOST_LIBRARIES})
target_include_directories(test_runner
                        PRIVATE
                        ${BOOST_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_SOURCE_DIR}
                        ${CMAKE_CURRENT_SOURCE_DIR}/..
                        ${CMAKE_CURRENT_SOURCE_DIR}/../../third_party/ut/include
                        )
# All unit tests
add_executable(unit_test_runner
    unit_test_runner.cpp
    test_AudioLoop.cpp)

target_link_libraries(unit_test_runner PRIVATE
                      ${BOOST_LIBRARIES})

target_include_directories(unit_test_runner
                        PRIVATE
                        ${BOOST_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_SOURCE_DIR}/../..
                        ${CMAKE_CURRENT_SOURCE_DIR}/../../../third_party/ut/include
                        )
# Subdirs
add_subdirectory(unit)
add_subdirectory(integration)
add_subdirectory(tools)
add_subdirectory(qt)

# Test runner
add_executable(test_runner
    test_runner.cpp
    unit/test_AudioMidiLoop_audio.cpp
    unit/test_AudioMidiLoop_midi.cpp
    unit/test_ObjectPool.cpp
    unit/test_BasicLoop.cpp
    integration/test_synced_BasicLoops.cpp
    )
target_link_libraries(test_runner PRIVATE
                      ${BOOST_LIBRARIES}
                      shoopdaloop_backend)
target_include_directories(test_runner
                        PRIVATE
                        ${BOOST_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_SOURCE_DIR}
                        ${CMAKE_CURRENT_SOURCE_DIR}/..
                        ${CMAKE_SOURCE_DIR}/third_party/ut/include
                        )
target_compile_definitions(test_runner PUBLIC -DMODULE_LOG_LEVEL=LOG_LEVEL_TRACE)
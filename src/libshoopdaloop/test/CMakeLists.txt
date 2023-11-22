# Test runner
add_executable(test_runner
    unit/test_AudioMidiLoop_audio.cpp
    unit/test_AudioMidiLoop_midi.cpp
    unit/test_ObjectPool.cpp
    unit/test_BasicLoop.cpp
    unit/test_DummyAudioSystem.cpp
    integration/test_libshoopdaloop_if.cpp
    integration/test_synced_BasicLoops.cpp
    integration/test_chains.cpp
    )
target_link_libraries(test_runner PRIVATE
                      shoopdaloop_internals
                      Catch2::Catch2WithMain
                      fmt::fmt
                      Threads::Threads)
target_include_directories(test_runner
                        PRIVATE
                        ${Boost_INCLUDE_DIR}
                        ${CMAKE_CURRENT_SOURCE_DIR}
                        ${CMAKE_CURRENT_SOURCE_DIR}/..
                        ${CMAKE_SOURCE_DIR}/../third_party/ut/include
                        )
target_compile_definitions(test_runner PUBLIC -DCOMPILE_LOG_LEVEL=LOG_LEVEL_TRACE)
target_compile_definitions(test_runner PUBLIC -DBOOST_UT_DISABLE_MODULE)
install(TARGETS test_runner
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})

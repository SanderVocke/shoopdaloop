add_executable(backend_runner backend_runner.cpp)
target_link_libraries(backend_runner PRIVATE
                      shoopdaloop
                      Boost::program_options)
target_include_directories(backend_runner
                        PRIVATE
                        ${CMAKE_SOURCE_DIR}/
                        ${CMAKE_SOURCE_DIR}/../third_party/lv2_evbuf
                        ${Boost_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_SOURCE_DIR}/../..
                        ${JACK_INCLUDE_DIRS}
                        ${LV2_INCLUDE_DIRS}
                        ${LILV_INCLUDE_DIRS}
                        ${CMAKE_SOURCE_DIR}/../third_party/base64/include
                        )
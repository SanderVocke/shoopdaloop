add_executable(backend_runner backend_runner.cpp)
message(INFO "Lilv libs: ${LILV_LIBRARIES}")
target_link_libraries(backend_runner PRIVATE
                      shoopdaloop_backend
                      Boost::program_options
                      lv2_evbuf
                      ${JACK_LIBRARIES}
                      ${LV2_LIBRARIES}
                      ${LILV_LIBRARIES})
target_include_directories(backend_runner
                        PRIVATE
                        ${CMAKE_SOURCE_DIR}/
                        ${CMAKE_SOURCE_DIR}/third_party/lv2_evbuf
                        ${Boost_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_SOURCE_DIR}/../..
                        ${JACK_INCLUDE_DIRS}
                        ${LV2_INCLUDE_DIRS}
                        ${LILV_INCLUDE_DIRS}
                        ${CMAKE_SOURCE_DIR}/third_party/base64/include
                        )
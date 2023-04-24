add_executable(backend_runner backend_runner.cpp)
message(INFO "Lilv libs: ${LILV_LIBRARIES}")
target_link_libraries(backend_runner PRIVATE
                      shoopdaloop_backend
                      Boost::program_options
                      ${JACK_LIBRARIES}
                      ${LV2_LIBRARIES}
                      ${LILV_LIBRARIES})
target_include_directories(backend_runner
                        PRIVATE
                        ${Boost_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_SOURCE_DIR}/../..
                        ${JACK_INCLUDE_DIRS}
                        ${LV2_INCLUDE_DIRS}
                        ${LILV_INCLUDE_DIRS}
                        )
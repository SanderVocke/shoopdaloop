add_executable(jack_loops jack_loops.cpp)
target_link_libraries(jack_loops PRIVATE
                      Boost::program_options
                      ${JACK_LIBRARIES})
target_include_directories(jack_loops
                        PRIVATE
                        ${Boost_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_SOURCE_DIR}/../..
                        ${JACK_INCLUDE_DIRS}
                        )
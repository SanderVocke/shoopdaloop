# Backend lib
add_library(shoopdaloop_backend SHARED shoopdaloop_backend.cpp)
target_link_libraries(shoopdaloop_backend PRIVATE
                      ${BOOST_LIBRARIES}
                      ${JACK_LIBRARIES})
target_include_directories(shoopdaloop_backend
                        PRIVATE
                        ${CMAKE_CURRENT_SOURCE_DIR}/..
                        ${JACK_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_BINARY_DIR}
                        ${BOOST_INCLUDE_DIRS}
                        )

# Autogen Python bindings
add_custom_target(
    __init__.py
    touch __init__.py
)
add_custom_target(
  shoopdaloop_backend.py
  ALL ctypesgen -lshoopdaloop_backend ${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop_backend.h ${CMAKE_CURRENT_SOURCE_DIR}/types.h -o shoopdaloop_backend.py
  DEPENDS shoopdaloop_backend __init__.py
  )
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
add_custom_command(
    OUTPUT __init__.py
    COMMAND touch __init__.py
    COMMENT "Generating __init__.py in frontend_interface folder"
    )
add_custom_target(
    frontend_python_module
    ALL
    DEPENDS __init__.py
    )

# Generate C bindings
add_custom_command(
  OUTPUT shoopdaloop_backend.py
  COMMAND ctypesgen -lshoopdaloop_backend ${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop_backend.h ${CMAKE_CURRENT_SOURCE_DIR}/types.h -o shoopdaloop_backend.py
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop_backend.h
  COMMENT "Generating C bindings for back-end."
  )
add_custom_target(
  backend_c_bindings
  ALL
  DEPENDS shoopdaloop_backend.py
  )
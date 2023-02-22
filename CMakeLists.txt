cmake_minimum_required(VERSION 3.22)
project(shoopdaloop_backend)

add_subdirectory(backend)

# Generate a python module file for generated .py files.
add_custom_command(
    OUTPUT __init__.py
    COMMAND touch __init__.py
    COMMENT "Generating __init__.py in build folder"
    )
add_custom_target(
    build_python_module
    ALL
    DEPENDS __init__.py
    )

# Generate javascript copy of enums
add_custom_command(
  OUTPUT types.js
  COMMAND python ${CMAKE_CURRENT_SOURCE_DIR}/lib/codegen/gen_js_enums.py ${CMAKE_CURRENT_BINARY_DIR}/types.js
  DEPENDS backend_c_bindings build_python_module
  COMMENT "Generating Javascript types."
  WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
  )
add_custom_target(
  javascript_types
  ALL
  DEPENDS types.js backend_c_bindings build_python_module
  )
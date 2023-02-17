cmake_minimum_required(VERSION 3.22)
project(shoopdaloop_backend)

add_subdirectory(backend)

# Generate a python module file for generated .py files.
add_custom_target(
    build_module
    ALL
    touch __init__.py
    BYPRODUCTS __init__.py
    )

# Generate javascript copy of enums
add_custom_target(
    types.js
    ALL
    python ${CMAKE_CURRENT_SOURCE_DIR}/lib/codegen/gen_js_enums.py ${CMAKE_CURRENT_BINARY_DIR}/types.js
    DEPENDS shoopdaloop_backend.py build_module
    WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}
)
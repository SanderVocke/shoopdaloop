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
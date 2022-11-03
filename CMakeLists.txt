cmake_minimum_required(VERSION 3.22)
project(shoopdaloop_backend)

add_subdirectory(backend)

add_custom_target(
    build_module
    ALL
    touch __init__.py
    BYPRODUCTS __init__.py
)
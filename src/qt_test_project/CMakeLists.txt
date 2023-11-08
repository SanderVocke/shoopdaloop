cmake_minimum_required(VERSION 3.22)
project(shoopdaloop)

# Set up language settings
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED YES)

find_package(Qt6 REQUIRED COMPONENTS Core Qml Gui Quick Widgets QuickTest)
qt_standard_project_setup()
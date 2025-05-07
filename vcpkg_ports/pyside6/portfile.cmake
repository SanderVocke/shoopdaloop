vcpkg_from_git(
    OUT_SOURCE_PATH SOURCE_PATH
    URL https://code.qt.io/pyside/pyside-setup
    REF 7055552166500415d7d472099d8429a2d46031d4
    FETCH_REF "v6.8.3"
)

x_vcpkg_get_python_packages(
    PYTHON_VERSION "3"
    REQUIREMENTS_FILE "${SOURCE_PATH}/requirements.txt"
    OUT_PYTHON_VAR PYTHON3_IN_VENV
)

vcpkg_cmake_configure(
    SOURCE_PATH "${SOURCE_PATH}"
    OPTIONS
        -DPython_EXECUTABLE="${PYTHON3_IN_VENV}"
)
vcpkg_cmake_install()
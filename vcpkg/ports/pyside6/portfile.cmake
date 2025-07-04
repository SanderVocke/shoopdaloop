vcpkg_from_git(
    OUT_SOURCE_PATH SOURCE_PATH
    URL https://code.qt.io/pyside/pyside-setup
    REF 7055552166500415d7d472099d8429a2d46031d4
    FETCH_REF "v6.8.3"
    PATCHES
        fix_example_icons_python_include.patch
        fix_shiboken_module_path.patch
        fix_python_debug_library_windows.patch
)

if(VCPKG_TARGET_IS_WINDOWS AND VCPKG_TARGET_IS_MINGW)
    if(VCPKG_TARGET_ARCHITECTURE STREQUAL "x64")
        set(ARCHIVE_NAME "libclang-release_140-based-windows-mingw_64-regular.7z")
        set(ARCHIVE_SHA512 "ac1d6b4751673a71b6e9c4d69e5e14e43f730290c8d01b0c658d7ad0451f104560bae1f406ca09607a4bdc6a78c4d46ef380be823fea5a06fabe325e9b6c86f4")
    else()
        message(FATAL_ERROR "Unsupported architecture for MinGW: ${VCPKG_TARGET_ARCHITECTURE}")
    endif()
elseif(VCPKG_TARGET_IS_WINDOWS)
    if(VCPKG_TARGET_ARCHITECTURE STREQUAL "x64")
        set(ARCHIVE_NAME "libclang-release_140-based-windows-vs2019_64.7z")
        set(ARCHIVE_SHA512 "ebc8f23969b6ee8a6a919f04918da2c4172b6826f6b3c91d1987a4787970fcd503d20b551a013fae0a4b0fa0131d1a2e645adc6b28b47371f949612509a568e1")
    else()
        message(FATAL_ERROR "Unsupported architecture for MSVC: ${VCPKG_TARGET_ARCHITECTURE}")
    endif()
elseif(VCPKG_TARGET_IS_LINUX)
    if(VCPKG_TARGET_ARCHITECTURE STREQUAL "x64")
        if(VCPKG_DETECTED_CMAKE_SYSTEM_NAME MATCHES ".*Ubuntu.*")
            set(ARCHIVE_NAME "libclang-release_140-based-linux-Ubuntu20.04-gcc9.3-x86_64.7z")
            set(ARCHIVE_SHA512 "9c31aab0f106a11339b892c240365e9f9143491af3f6b74cfadaa7e74be4e7753d617fea7a0b6936591b42954aa3b9f65699e55a2a118aeaa94d3215e105e72b")
        else()
            set(ARCHIVE_NAME "libclang-release_140-based-linux-Rhel8.2-gcc9.2-x86_64.7z")
            set(ARCHIVE_SHA512 "cd1d76ca70e68f4fcde0b7d7dbb0264db9817e357234c7bb5c031115e15b3850c03e903af0fdc3d78526362383f69ff7b0198cb81eca688941ee04959cd08d41")
        endif()
    else()
        message(FATAL_ERROR "Unsupported architecture for Linux: ${VCPKG_TARGET_ARCHITECTURE}")
    endif()
elseif(VCPKG_TARGET_IS_OSX)
    set(ARCHIVE_NAME "libclang-release_20.1.3-based-macos-universal.7z")
    set(ARCHIVE_SHA512 "49106f594bb412812d879f2be419b0dcbca24b36197bf6d25bf4fe112f3a9ef9f3e8e836b78c486a6dd96c76d9884022325094ece3cffe92658652c877fc7a3b")
else()
    message(FATAL_ERROR "Unsupported platform (no Qt-supplied prebuilt libclang available)")
endif()

set(URL "https://download.qt.io/development_releases/prebuilt/libclang/${ARCHIVE_NAME}")

vcpkg_download_distfile(
    ARCHIVE_PATH
    URLS ${URL}
    FILENAME ${ARCHIVE_NAME}
    SHA512 "${ARCHIVE_SHA512}"
)

set(LIBCLANG_EXTRACTED "${CURRENT_BUILDTREES_DIR}/libclang-downloaded")
if(EXISTS "${LIBCLANG_EXTRACTED}")
    file(REMOVE_RECURSE "${LIBCLANG_EXTRACTED}")
endif()
vcpkg_extract_archive(
    ARCHIVE ${ARCHIVE_PATH}
    DESTINATION ${LIBCLANG_EXTRACTED}
)
set(ENV{LLVM_INSTALL_DIR} "${LIBCLANG_EXTRACTED}/libclang")

if(VCPKG_TARGET_IS_WINDOWS)
        set(PYTHON3_INTERPRETER_DEBUG "${CURRENT_INSTALLED_DIR}/debug/tools/python3/python_d.exe")
        set(PYTHON3_INTERPRETER_RELEASE "${CURRENT_INSTALLED_DIR}/tools/python3/python.exe")
else()
        set(PYTHON3_INTERPRETER_DEBUG "${CURRENT_INSTALLED_DIR}/debug/tools/python3/python3d")
        set(PYTHON3_INTERPRETER_RELEASE "${CURRENT_INSTALLED_DIR}/tools/python3/python3")
endif()

set(PYTHON3_LIBDIR_DEBUG "${CURRENT_INSTALLED_DIR}/debug/lib")
set(PYTHON3_LIBDIR_RELEASE "${CURRENT_INSTALLED_DIR}/lib")

execute_process(
    COMMAND ${PYTHON3_INTERPRETER_DEBUG} -c "import sysconfig; print(sysconfig.get_path('include'))"
    OUTPUT_VARIABLE PYTHON3_INCLUDE_DEBUG
    OUTPUT_STRIP_TRAILING_WHITESPACE
)
execute_process(
    COMMAND ${PYTHON3_INTERPRETER_RELEASE} -c "import sysconfig; print(sysconfig.get_path('include'))"
    OUTPUT_VARIABLE PYTHON3_INCLUDE_RELEASE
    OUTPUT_STRIP_TRAILING_WHITESPACE
)

vcpkg_cmake_configure(
    SOURCE_PATH "${SOURCE_PATH}" 
    OPTIONS
        -DMODULES=Core\;Gui\;Widgets\;Network\;Concurrent\;DBus\;OpenGL\;OpenGLWidgets\;PrintSupport\;Qml\;Quick\;QuickControls2\;QuickTest\;QuickWidgets\;Xml\;Test\;Svg\;SvgWidgets
        -DFORCE_LIMITED_API=OFF
        -DDISABLE_PYI=YES
        -DBUILD_TESTS=NO
        -DNO_QT_TOOLS=yes
    OPTIONS_DEBUG
        -DPython_EXECUTABLE="${PYTHON3_INTERPRETER_DEBUG}"
        -DPython_INCLUDE_DIRS="${PYTHON3_INCLUDE_DEBUG}"
        -DPython_LIBRARIES="${PYTHON3_LIBDIR_DEBUG}"
    OPTIONS_RELEASE
        -DPython_EXECUTABLE="${PYTHON3_INTERPRETER_RELEASE}"
        -DPython_INCLUDE_DIRS="${PYTHON3_INCLUDE_RELEASE}"
        -DPython_LIBRARIES="${PYTHON3_LIBDIR_RELEASE}"
)
vcpkg_cmake_build()
vcpkg_cmake_install()
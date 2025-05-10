vcpkg_from_git(
    OUT_SOURCE_PATH SOURCE_PATH
    URL https://code.qt.io/pyside/pyside-setup
    REF 7055552166500415d7d472099d8429a2d46031d4
    FETCH_REF "v6.8.3"
    PATCHES
        fix_example_icons_python_include.patch
        fix_shiboken_module_path.patch
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
    set(ARCHIVE_NAME "libclang-release_140-based-macos-universal.7z")
    set(ARCHIVE_SHA512 "59c776759987c15c9719473e5459d246bb5ee92e64881b50de4ee13d9875499909afbf7852e5ba3a5692fda017f3fc0ebcf499204178e53091705d41ea69e406")
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

vcpkg_get_vcpkg_installed_python (VCPKG_PYTHON3)
execute_process(
    COMMAND ${VCPKG_PYTHON3} -c "import sysconfig; print(sysconfig.get_path('include'))"
    OUTPUT_VARIABLE PYTHON3_INCLUDE
    OUTPUT_STRIP_TRAILING_WHITESPACE
)
execute_process(
    COMMAND ${VCPKG_PYTHON3} -c "import sysconfig; print(sysconfig.get_config_var('LIBDIR'))"
    OUTPUT_VARIABLE PYTHON3_LIBDIR
    OUTPUT_STRIP_TRAILING_WHITESPACE
)

set(MAYBE_DEBUG_SUBFOLDER "")
if(VCPKG_BUILD_TYPE STREQUAL "Debug")
    set(MAYBE_DEBUG_SUBFOLDER "debug/")
endif()

vcpkg_cmake_configure(
    SOURCE_PATH "${SOURCE_PATH}" 
    OPTIONS
        -DPython_EXECUTABLE="${VCPKG_PYTHON3}"
        -DPython_INCLUDE_DIRS="${PYTHON3_INCLUDE}"
        -DPython_LIBRARIES="${PYTHON3_LIBDIR}"
        #-DMODULES=Core\;Gui\;Widgets\;Help\;Network\;Concurrent\;DBus\;Designer\;OpenGL\;OpenGLWidgets\;PrintSupport\;Qml\;Quick\;QuickControls2\;QuickTest\;QuickWidgets\;Xml\;Test\;Sql\;Svg\;SvgWidgets\;UiTools
        #-DMODULES=Core\;Gui\;Qml\;Quick\;QuickControls2\;Network\;OpenGL
        -DMODULES=Core\;Gui\;Widgets\;Network\;Concurrent\;DBus\;OpenGL\;OpenGLWidgets\;PrintSupport\;Qml\;Quick\;QuickControls2\;QuickTest\;QuickWidgets\;Xml\;Test\;Sql\;Svg\;SvgWidgets
        -DFORCE_LIMITED_API=yes
)
vcpkg_cmake_build()
vcpkg_cmake_install()
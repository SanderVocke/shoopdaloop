if(VCPKG_TARGET_IS_WINDOWS)
    vcpkg_check_linkage(ONLY_STATIC_LIBRARY)
endif()

vcpkg_from_github(
    OUT_SOURCE_PATH SOURCE_PATH
    REPO catchorg/Catch2
    REF v${VERSION}
    SHA512 a95495142f915d6e9c2a23e80fe360343e9097680066a2f9d3037a070ba5f81ee5559a0407cc9e972dc2afae325873f1fc7ea07a64012c0f01aac6e549f03e3f
    HEAD_REF devel
    PATCHES
        fix-install-path.patch
)

vcpkg_check_features(OUT_FEATURE_OPTIONS OPTIONS
    FEATURES
        thread-safe-assertions CATCH_CONFIG_EXPERIMENTAL_THREAD_SAFE_ASSERTIONS
)

vcpkg_cmake_configure(
    SOURCE_PATH "${SOURCE_PATH}"
    OPTIONS
        ${OPTIONS}
        -DCATCH_INSTALL_DOCS=OFF
        -DCMAKE_CXX_STANDARD=17
        # Force /MD for both Debug and Release configurations. This is needed
        # because our Rust crate (refilling_pool) produces a staticlib that
        # always links against the release MSVCRT, and our backend links to
        # it. Using /MD for Catch2 prevents LNK2038 / CRT mismatch errors
        # when the test runner links against both Catch2 (from vcpkg) and
        # backend internals (which transitively depend on the Rust staticlib).
        -DCMAKE_MSVC_RUNTIME_LIBRARY=MultiThreadedDLL
)

vcpkg_cmake_install()

vcpkg_cmake_config_fixup(CONFIG_PATH lib/cmake/Catch2)
vcpkg_fixup_pkgconfig()
vcpkg_replace_string("${CURRENT_PACKAGES_DIR}/lib/pkgconfig/catch2-with-main.pc" [["-L${libdir}"]] [["-L${libdir}/manual-link"]])
if(NOT VCPKG_BUILD_TYPE)
    vcpkg_replace_string("${CURRENT_PACKAGES_DIR}/debug/lib/pkgconfig/catch2-with-main.pc" [["-L${libdir}"]] [["-L${libdir}/manual-link"]])
endif()

file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/debug/include")
file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/debug/share")

# We remove these folders because they are empty and cause warnings on the library installation
file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/include/catch2/benchmark/internal")
file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/include/catch2/generators/internal")

file(WRITE "${CURRENT_PACKAGES_DIR}/include/catch.hpp" "#include <catch2/catch_all.hpp>")
vcpkg_install_copyright(FILE_LIST "${SOURCE_PATH}/LICENSE.txt")

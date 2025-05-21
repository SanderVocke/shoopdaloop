vcpkg_from_github(
    OUT_SOURCE_PATH SOURCE_PATH
    REPO SanderVocke/shoop-zita-resampler
    REF "v1.8.0-5"
    SHA512 1e76aaf56f16e489a00c534fba7bfe8000f3cd45f8cf5d527c64351bf4f3556bc0b8753ccfdd93c876e3e97ddf7c2aa96b64a4d7222d12a2bafaa5c8914ea9cb
    HEAD_REF main
)

vcpkg_cmake_configure(
    SOURCE_PATH "${SOURCE_PATH}"
)
vcpkg_cmake_install()
file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/debug/include")

set(VCPKG_POLICY_SKIP_COPYRIGHT_CHECK enabled)

vcpkg_copy_pdbs()
vcpkg_cmake_config_fixup(
    CONFIG_PATH "share/zita-resampler/cmake"
)
# vcpkg_fixup_pkgconfig()
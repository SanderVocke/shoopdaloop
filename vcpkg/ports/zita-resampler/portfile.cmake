vcpkg_from_github(
    OUT_SOURCE_PATH SOURCE_PATH
    REPO SanderVocke/shoop-zita-resampler
    REF "v1.8.0-6"
    SHA512 c55a8c28919a307dac5d6fe1f68bf1cd321331f68ca7d4c3d0a3f6928caa65822494aa793e617eac98d983d44ff4a332424c0e20b66fad512df70e35d59b0f25
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
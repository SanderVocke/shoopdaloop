set(VCPKG_TARGET_ARCHITECTURE x64)
set(VCPKG_CRT_LINKAGE dynamic)
set(VCPKG_LIBRARY_LINKAGE dynamic)
set(VCPKG_CMAKE_SYSTEM_NAME Linux)

# Disable precompiled headers - they take too much disk space in GHA runners
if(PORT MATCHES "^qt")
  message(STATUS "Disabling precompiled headers for ${PORT}")
  set(VCPKG_CMAKE_CONFIGURE_OPTIONS "-DBUILD_WITH_PCH=OFF")
endif()

# libsystemd forces -Werror=override-init in its own build args, which trips on
# errno constants aliased by glibc headers newer than this pinned version. A bare
# -Wno-error cannot undo a specific -Werror=<w>; only the matching -Wno-error=<w>,
# appended after, does. Scoped to the port so unrelated ABI hashes stay stable.
if(PORT STREQUAL "libsystemd")
  set(VCPKG_C_FLAGS "${VCPKG_C_FLAGS} -Wno-error=override-init")
  set(VCPKG_CXX_FLAGS "${VCPKG_CXX_FLAGS} -Wno-error=override-init")
endif()
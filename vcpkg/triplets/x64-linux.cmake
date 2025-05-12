set(VCPKG_TARGET_ARCHITECTURE x64)
set(VCPKG_CRT_LINKAGE dynamic)
set(VCPKG_LIBRARY_LINKAGE dynamic)
set(VCPKG_CMAKE_SYSTEM_NAME Linux)

# Disable precompiled headers - they take too much disk space in GHA runners
if(PORT MATCHES "^qt")
  message(STATUS "Disabling precompiled headers for ${PORT}")
  set(VCPKG_CMAKE_CONFIGURE_OPTIONS "-DBUILD_WITH_PCH=OFF")
endif()
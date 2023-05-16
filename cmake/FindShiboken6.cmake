find_package(PkgConfig REQUIRED QUIET)

pkg_search_module(
  SHIBOKEN6
  REQUIRED
    shiboken6
  )

if(${SHIBOKEN6_FOUND})
    message(STATUS "Found Shiboken6")
else()
    message(ERROR "Could not find Shiboken6")
endif()
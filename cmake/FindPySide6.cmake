find_package(PkgConfig REQUIRED QUIET)

pkg_search_module(
  PYSIDE6
  REQUIRED
    pyside6
  )

if(${PYSIDE6_FOUND})
    message(STATUS "Found PySide6")
else()
    message(ERROR "Could not find PySide6")
endif()
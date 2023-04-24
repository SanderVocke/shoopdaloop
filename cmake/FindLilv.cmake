find_package(PkgConfig)
pkg_check_modules(PC_LILV QUIET lilv-0)
find_path(LILV_INCLUDE_DIRS NAMES lilv/lilv.h HINTS ${PC_LILV_INCLUDE_DIRS})
find_library(LILV_LIBRARIES NAMES lilv-0 HINTS ${PC_LILV_LIBRARY_DIRS})

set(LILV_DEFINITIONS ${PC_LILV_CFLAGS_OTHER})

include(FindPackageHandleStandardArgs)
find_package_handle_standard_args(Lilv DEFAULT_MSG LILV_LIBRARIES LILV_INCLUDE_DIRS)
mark_as_advanced(LILV_LIBRARIES LILV_INCLUDE_DIRS LILV_DEFINITIONS)

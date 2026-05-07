#.rst:
# FindB64
# -------
#
# Find b64/libb64 library
#
# Try to find the b64/libb64 base64 encoding/decoding library.
# The following values are defined
#
# ::
#
#   B64_FOUND         - True if b64 is available
#   B64_INCLUDE_DIRS  - Include directories for b64
#   B64_LIBRARIES     - List of libraries for b64
#   B64_DEFINITIONS   - List of definitions for b64
#
#=============================================================================

include(FeatureSummary)
set_package_properties(b64 PROPERTIES
   URL "https://sourceforge.net/projects/libb64/"
   DESCRIPTION "ANSI C routines for fast base64 encoding/decoding")

find_package(PkgConfig QUIET)
if(PkgConfig_FOUND)
    pkg_check_modules(PC_B64 QUIET b64 libb64)
endif()

# Standard search paths
find_path(B64_INCLUDE_DIRS NAMES b64/cencode.h b64/cdecode.h
    HINTS ${PC_B64_INCLUDE_DIRS} $ENV{B64_ROOT}/include $ENV{B64_PATH}/include $ENV{CMAKE_INCLUDE_PATH}
    PATH_SUFFIXES include)

find_library(B64_LIBRARIES NAMES b64 libb64
    HINTS ${PC_B64_LIBRARY_DIRS} $ENV{B64_ROOT}/lib $ENV{B64_PATH}/lib $ENV{CMAKE_LIBRARY_PATH}
    PATH_SUFFIXES lib)

# Fallback: search in Nix store paths (for NixOS environments)
if(NOT B64_INCLUDE_DIRS OR NOT B64_LIBRARIES)
    # Find libb64 in Nix store by globbing
    if(DEFINED ENV{NIX_STORE})
        file(GLOB B64_NIX_CANDIDATES "$ENV{NIX_STORE}/*libb64*")
    else()
        file(GLOB B64_NIX_CANDIDATES "/nix/store/*libb64*")
    endif()
    foreach(candidate ${B64_NIX_CANDIDATES})
        if(IS_DIRECTORY "${candidate}" AND NOT B64_INCLUDE_DIRS)
            if(EXISTS "${candidate}/include/b64/cencode.h")
                set(B64_INCLUDE_DIRS "${candidate}/include")
            endif()
        endif()
        if(IS_DIRECTORY "${candidate}" AND NOT B64_LIBRARIES)
            if(EXISTS "${candidate}/lib/libb64.a")
                set(B64_LIBRARIES "${candidate}/lib/libb64.a")
            elseif(EXISTS "${candidate}/lib/libb64.so")
                set(B64_LIBRARIES "${candidate}/lib/libb64.so")
            endif()
        endif()
    endforeach()
endif()

set(B64_DEFINITIONS ${PC_B64_CFLAGS_OTHER})

include(FindPackageHandleStandardArgs)
find_package_handle_standard_args(B64 DEFAULT_MSG B64_LIBRARIES B64_INCLUDE_DIRS)

if(B64_FOUND AND NOT TARGET b64::b64)
    # Determine if library is static or shared
    get_filename_component(B64_LIB_EXT "${B64_LIBRARIES}" EXT)
    if(B64_LIB_EXT STREQUAL ".a" OR B64_LIB_EXT STREQUAL ".lib")
        add_library(b64::b64 STATIC IMPORTED)
    else()
        add_library(b64::b64 SHARED IMPORTED)
    endif()
    set_target_properties(b64::b64 PROPERTIES
        IMPORTED_LOCATION "${B64_LIBRARIES}"
        INTERFACE_INCLUDE_DIRECTORIES "${B64_INCLUDE_DIRS}"
    )
endif()

mark_as_advanced(B64_LIBRARIES B64_INCLUDE_DIRS B64_DEFINITIONS)
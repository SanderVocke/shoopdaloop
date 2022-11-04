#.rst:
# FindJACK
# -------
#
# Find jack library
#
# Try to find jack library. The following values are defined
#
# ::
#
#   JACK_FOUND         - True if jack is available
#   JACK_INCLUDE_DIRS  - Include directories for jack
#   JACK_LIBRARIES     - List of libraries for jack
#   JACK_DEFINITIONS   - List of definitions for jack
#
#=============================================================================
# Copyright (c) 2015 Jari Vetoniemi
#
# Distributed under the OSI-approved BSD License (the "License");
#
# This software is distributed WITHOUT ANY WARRANTY; without even the
# implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
# See the License for more information.
#=============================================================================

include(FeatureSummary)
set_package_properties(jack PROPERTIES
   URL "http://www.jackaudio.org/"
   DESCRIPTION "JACK Audio Connection Kit")

find_package(PkgConfig)
pkg_check_modules(PC_JACK QUIET jack)
find_path(JACK_INCLUDE_DIRS NAMES jack/jack.h HINTS ${PC_JACK_INCLUDE_DIRS})
find_library(JACK_LIBRARIES NAMES jack HINTS ${PC_JACK_LIBRARY_DIRS})

set(JACK_DEFINITIONS ${PC_JACK_CFLAGS_OTHER})

include(FindPackageHandleStandardArgs)
find_package_handle_standard_args(jack DEFAULT_MSG JACK_LIBRARIES JACK_INCLUDE_DIRS)
mark_as_advanced(JACK_LIBRARIES JACK_INCLUDE_DIRS JACK_DEFINITIONS)

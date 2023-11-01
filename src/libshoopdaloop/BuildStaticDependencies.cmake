set(STATIC_DEPS_PREFIX ${CMAKE_CURRENT_BINARY_DIR}/static-deps)

##################
## fmt
##################
add_library(fmtlib_static STATIC ${CMAKE_SOURCE_DIR}/../third_party/fmt/src/format.cc)
target_include_directories(fmtlib_static PUBLIC ${CMAKE_SOURCE_DIR}/../third_party/fmt/include)
set(fmt_LIBRARY fmtlib_static)
set(fmt_INCLUDE_DIR ${CMAKE_SOURCE_DIR}/../third_party/fmt/include)


###################
# "Build" spdlog (just a single-header)
###################
set(SPDLOG_INCLUDE_DIR ${CMAKE_SOURCE_DIR}/../third_party/spdlog/include)


###################
# Build Lilv (including serd, sord)
###################
include(ExternalProject)

ExternalProject_Add(lv2
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/lv2
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/lv2_build
  CONFIGURE_COMMAND
    python -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    python -m mesonbuild.mesonmain compile lv2
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install --prefix=${STATIC_DEPS_PREFIX} lv2
)

ExternalProject_Add(serd
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/serd
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/serd_build
  CONFIGURE_COMMAND
    python -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    python -m mesonbuild.mesonmain compile serd-0
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install --prefix=${STATIC_DEPS_PREFIX} serd-0
)

ExternalProject_Add(sord
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/sord
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/sord_build
  CONFIGURE_COMMAND
    python -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    python -m mesonbuild.mesonmain compile sord-0
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install --prefix=${STATIC_DEPS_PREFIX} sord-0
)

ExternalProject_Add(lilv
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/lilv
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/lilv_build
  CONFIGURE_COMMAND
    python -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    python -m mesonbuild.mesonmain compile lilv-0
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install --prefix=${STATIC_DEPS_PREFIX} lilv-0
)
set(LILV_INCLUDE_DIRS ${CMAKE_SOURCE_DIR}/../third_party/lilv/include)
set(LILV_LIBRARIES ${CMAKE_CURRENT_BINARY_DIR}/lilv_build/liblilv-0.a ${CMAKE_CURRENT_BINARY_DIR}/serd_build/libserd-0.a ${CMAKE_CURRENT_BINARY_DIR}/sord_build/libsord-0.a)
set(DEPEND_ON_LILV serd sord lilv)
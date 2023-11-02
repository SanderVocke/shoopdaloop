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
    python -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install
)

find_path (STATIC_PKGCONFIGS
  NAMES ${STATIC_DEPS_PREFIX}/lib64/pkgconfig/lv2.pc ${STATIC_DEPS_PREFIX}/lib/pkgconfig/lv2.pc)

ExternalProject_Add(serd
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/serd
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/serd_build
  CONFIGURE_COMMAND
    PKG_CONFIG_PATH=${STATIC_PKGCONFIGS} python -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    python -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install
  DEPENDS lv2
)

ExternalProject_Add(sord
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/sord
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/sord_build
  CONFIGURE_COMMAND
    PKG_CONFIG_PATH=${STATIC_PKGCONFIGS} python -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    python -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install
  DEPENDS lv2 serd
)

ExternalProject_Add(sratom
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/sratom
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/sratom_build
  CONFIGURE_COMMAND
    PKG_CONFIG_PATH=${STATIC_PKGCONFIGS} python -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    python -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install
  DEPENDS lv2 serd sord
)

ExternalProject_Add(lilv
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/lilv
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/lilv_build
  CONFIGURE_COMMAND
    PKG_CONFIG_PATH=${STATIC_PKGCONFIGS} python -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    python -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install
  DEPENDS lv2 serd sord sratom
)
set(LILV_INCLUDE_DIRS ${CMAKE_SOURCE_DIR}/../third_party/lilv/include)
set(LILV_LIBRARIES ${CMAKE_CURRENT_BINARY_DIR}/lilv_build/liblilv-0.a ${CMAKE_CURRENT_BINARY_DIR}/serd_build/libserd-0.a ${CMAKE_CURRENT_BINARY_DIR}/sord_build/libsord-0.a)
set(LV2_INCLUDE_DIRS ${STATIC_DEPS_PREFIX}/include)
set(DEPEND_ON_LILV lv2 serd sord sratom lilv)
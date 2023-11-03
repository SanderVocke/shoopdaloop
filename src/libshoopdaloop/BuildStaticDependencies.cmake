set(STATIC_DEPS_PREFIX ${CMAKE_CURRENT_BINARY_DIR}/static-deps)
set(PKGDIR_FILE ${CMAKE_CURRENT_BINARY_DIR}/pkgdir)
set(WITH_PKGCONF_CMD ${RUN_WITH_ENV_CMD} PKG_CONFIG_PATH=${PKGDIR_FILE} -- )
set(FIND_PKGCONF_DIRS ${GLOB_CMD} directory separator ":" ${STATIC_DEPS_PREFIX}/**/*.pc > ${PKGDIR_FILE})

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
    COMMAND ${FIND_PKGCONF_DIRS}
)

ExternalProject_Add(serd
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/serd
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/serd_build
  CONFIGURE_COMMAND
    ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    python -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    python -m mesonbuild.mesonmain install
    COMMAND ${FIND_PKGCONF_DIRS}
  DEPENDS lv2
)

ExternalProject_Add(sord
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/sord
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/sord_build
  CONFIGURE_COMMAND
    ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
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
    ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
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
    ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddefault_library=static --prefix=${STATIC_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
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
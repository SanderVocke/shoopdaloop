set(PKGDIR_FILE ${CMAKE_CURRENT_BINARY_DIR}/pkgdir)
set(WITH_PKGCONF_CMD ${RUN_WITH_ENV_CMD} PKG_CONFIG_PATH=${PKGDIR_FILE} -- )
set(FIND_PKGCONF_DIRS ${GLOB_CMD} directory separator ":" ${EXTERNAL_DEPS_PREFIX}/**/*.pc > ${PKGDIR_FILE})

set(MESON_ENV ${CMAKE_COMMAND} -E env CXX=${CMAKE_CXX_COMPILER} CC=${CMAKE_C_COMPILER})
set(MESON_OPTS --default-library=shared --prefix=${EXTERNAL_DEPS_PREFIX})

###################
# Build Lilv (including serd, sord)
###################
include(ExternalProject)

set(LILV_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/lilv_build)
set(SERD_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/serd_build)
set(SORD_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/sord_build)
set(SRATOM_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/sratom_build)
if(WIN32)
  set(LILV_LIB ${LILV_BUILD_DIR}/lilv-0.dll)
  set(SERD_LIB ${SERD_BUILD_DIR}/serd-0.dll)
  set(SORD_LIB ${SORD_BUILD_DIR}/sord-0.dll)
  set(SRATOM_LIB ${SRATOM_BUILD_DIR}/sratom-0.dll)
  set(LILV_OUTPUTS ${LILV_LIB})
  set(SERD_OUTPUTS ${SERD_LIB})
  set(SORD_OUTPUTS ${SORD_LIB})
  set(SRATOM_OUTPUTS ${SRATOM_LIB})
  set(GLOB_LIBS_PATTERN "*.dll")
else()
  set(LILV_LIB ${LILV_BUILD_DIR}/liblilv-0.so)
  set(SERD_LIB ${SERD_BUILD_DIR}/libserd-0.so)
  set(SORD_LIB ${SORD_BUILD_DIR}/libsord-0.so)
  set(SRATOM_LIB ${SRATOM_BUILD_DIR}/libsratom-0.so)
  set(LILV_OUTPUTS ${LILV_LIB})
  set(SERD_OUTPUTS ${SERD_LIB})
  set(SORD_OUTPUTS ${SORD_LIB})
  set(SRATOM_OUTPUTS ${SRATOM_LIB})
  set(GLOB_LIBS_PATTERN "*.so*")
endif()

ExternalProject_Add(lv2_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/lv2
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/lv2_build
  CONFIGURE_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled ${MESON_OPTS} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${FIND_PKGCONF_DIRS}
)

ExternalProject_Add(serd_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/serd
  BINARY_DIR ${SERD_BUILD_DIR}
  CONFIGURE_COMMAND
    ${MESON_ENV} ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled -Dtests=disabled ${MESON_OPTS} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${CMAKE_COMMAND} -E copy ${SERD_LIB} ${CMAKE_CURRENT_BINARY_DIR}
  DEPENDS lv2_proj
  BUILD_BYPRODUCTS ${SERD_OUTPUTS}
)

ExternalProject_Add(sord_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/sord
  BINARY_DIR ${SORD_BUILD_DIR}
  CONFIGURE_COMMAND
    ${MESON_ENV} ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled -Dtests=disabled ${MESON_OPTS} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${CMAKE_COMMAND} -E copy ${SORD_LIB} ${CMAKE_CURRENT_BINARY_DIR}
  DEPENDS lv2_proj serd_proj
  BUILD_BYPRODUCTS ${SORD_OUTPUTS}
)

ExternalProject_Add(sratom_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/sratom
  BINARY_DIR ${SRATOM_BUILD_DIR}
  CONFIGURE_COMMAND
    ${MESON_ENV} ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled -Dtests=disabled ${MESON_OPTS} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${CMAKE_COMMAND} -E copy ${SRATOM_LIB} ${CMAKE_CURRENT_BINARY_DIR}
  DEPENDS lv2_proj serd_proj sord_proj
  BUILD_BYPRODUCTS ${SRATOM_OUTPUTS}
)

ExternalProject_Add(lilv_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/lilv
  BINARY_DIR ${LILV_BUILD_DIR}
  CONFIGURE_COMMAND
    ${MESON_ENV} ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled -Dtests=disabled ${MESON_OPTS} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${CMAKE_COMMAND} -E copy ${LILV_LIB} ${CMAKE_CURRENT_BINARY_DIR}
  DEPENDS lv2_proj serd_proj sord_proj sratom_proj
  BUILD_BYPRODUCTS ${LILV_OUTPUTS}
)

set(LV2_INCLUDE_DIRS ${EXTERNAL_DEPS_PREFIX}/include)
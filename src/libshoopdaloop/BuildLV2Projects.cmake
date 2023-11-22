set(PKGDIR_FILE ${CMAKE_CURRENT_BINARY_DIR}/pkgdir)
set(WITH_PKGCONF_CMD ${RUN_WITH_ENV_CMD} PKG_CONFIG_PATH=${PKGDIR_FILE} -- )
set(FIND_PKGCONF_DIRS ${GLOB_CMD} directory separator ":" ${EXTERNAL_DEPS_PREFIX}/**/*.pc > ${PKGDIR_FILE})

set(MESON_ENV ${CMAKE_COMMAND} -E env CXX=${CMAKE_CXX_COMPILER} CC=${CMAKE_C_COMPILER})
set(MESON_OPTS --default-library=static --prefer-static --prefix=${EXTERNAL_DEPS_PREFIX})

add_compile_definitions(-DLILV_STATIC)

###################
# Build Lilv (including serd, sord)
###################
include(ExternalProject)

set(LILV_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/lilv_build)
set(SERD_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/serd_build)
set(SORD_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/sord_build)
set(SRATOM_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/sratom_build)
if(WIN32)
  set(LILV_LIB ${LILV_BUILD_DIR}/liblilv-0.a)
  set(SERD_LIB ${SERD_BUILD_DIR}/libserd-0.a)
  set(SORD_LIB ${SORD_BUILD_DIR}/libsord-0.a)
  set(SRATOM_LIB ${SRATOM_BUILD_DIR}/libsratom-0.a)
  set(LILV_OUTPUTS ${LILV_LIB})
  set(SERD_OUTPUTS ${SERD_LIB})
  set(SORD_OUTPUTS ${SORD_LIB})
  set(SRATOM_OUTPUTS ${SRATOM_LIB})
else()
  set(LILV_LIB ${LILV_BUILD_DIR}/liblilv-0.a)
  set(SERD_LIB ${SERD_BUILD_DIR}/libserd-0.a)
  set(SORD_LIB ${SORD_BUILD_DIR}/libsord-0.a)
  set(SRATOM_LIB ${SRATOM_BUILD_DIR}/libsratom-0.a)
  set(LILV_OUTPUTS ${LILV_LIB})
  set(SERD_OUTPUTS ${SERD_LIB})
  set(SORD_OUTPUTS ${SORD_LIB})
  set(SRATOM_OUTPUTS ${SRATOM_LIB})
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
    COMMAND ${FIND_PKGCONF_DIRS}
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
  DEPENDS lv2_proj serd_proj sord_proj sratom_proj
  BUILD_BYPRODUCTS ${LILV_OUTPUTS}
)

add_library(lilv STATIC IMPORTED)
add_library(serd STATIC IMPORTED)
add_library(sord STATIC IMPORTED)
add_library(sratom STATIC IMPORTED)
set_property(TARGET lilv PROPERTY IMPORTED_LOCATION ${LILV_LIB})
set_property(TARGET serd PROPERTY IMPORTED_LOCATION ${SERD_LIB})
set_property(TARGET sord PROPERTY IMPORTED_LOCATION ${SORD_LIB})
set_property(TARGET sratom PROPERTY IMPORTED_LOCATION ${SRATOM_LIB})
add_dependencies(lilv lilv_proj)
add_dependencies(serd serd_proj)
add_dependencies(sord sord_proj)
add_dependencies(sratom sratom_proj)

set(LILV_INCLUDE_DIRS ${CMAKE_SOURCE_DIR}/../third_party/lilv/include)
set(DEPEND_ON_LILV lilv sord serd sratom)
set(LV2_INCLUDE_DIRS ${EXTERNAL_DEPS_PREFIX}/include)

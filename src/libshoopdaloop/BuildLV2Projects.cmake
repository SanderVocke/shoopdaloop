set(PKGDIR_FILE ${CMAKE_CURRENT_BINARY_DIR}/pkgdir)
set(WITH_PKGCONF_CMD ${RUN_WITH_ENV_CMD} PKG_CONFIG_PATH=${PKGDIR_FILE} -- )
set(FIND_PKGCONF_DIRS ${GLOB_CMD} directory separator ":" ${EXTERNAL_DEPS_PREFIX}/**/*.pc > ${PKGDIR_FILE})

set(MESON_ENV ${CMAKE_COMMAND} -E env CXX=${CMAKE_CXX_COMPILER} CC=${CMAKE_C_COMPILER})

###################
# Build Lilv (including serd, sord)
###################
include(ExternalProject)

if(WIN32)
  set(LILV_LIB ${CMAKE_CURRENT_BINARY_DIR}/lilv_build/lilv-0.dll)
  set(SERD_LIB ${CMAKE_CURRENT_BINARY_DIR}/serd_build/serd-0.dll)
  set(SORD_LIB ${CMAKE_CURRENT_BINARY_DIR}/sord_build/sord-0.dll)
  set(SRATOM_LIB ${CMAKE_CURRENT_BINARY_DIR}/sratom_build/sratom-0.dll)
  set(LILV_IMPLIB ${CMAKE_CURRENT_BINARY_DIR}/lilv_build/lilv.lib)
  set(SERD_IMPLIB ${CMAKE_CURRENT_BINARY_DIR}/serd_build/serd.lib)
  set(SORD_IMPLIB ${CMAKE_CURRENT_BINARY_DIR}/sord_build/sord.lib)
  set(SRATOM_IMPLIB ${CMAKE_CURRENT_BINARY_DIR}/sratom_build/sratom.lib)
  set(LILV_OUTPUTS ${LILV_LIB} ${LILV_IMPLIB})
  set(SERD_OUTPUTS ${SERD_LIB} ${SERD_IMPLIB})
  set(SORD_OUTPUTS ${SORD_LIB} ${SORD_IMPLIB})
  set(SRATOM_OUTPUTS ${SRATOM_LIB} ${SRATOM_IMPLIB})
else()
  set(LILV_LIB ${CMAKE_CURRENT_BINARY_DIR}/lilv_build/liblilv-0.so)
  set(SERD_LIB ${CMAKE_CURRENT_BINARY_DIR}/serd_build/libserd-0.so)
  set(SORD_LIB ${CMAKE_CURRENT_BINARY_DIR}/sord_build/libsord-0.so)
  set(SRATOM_LIB ${CMAKE_CURRENT_BINARY_DIR}/sratom_build/libsratom-0.so)
  set(LILV_OUTPUTS ${LILV_LIB})
  set(SERD_OUTPUTS ${SERD_LIB})
  set(SORD_OUTPUTS ${SORD_LIB})
  set(SRATOM_OUTPUTS ${SRATOM_LIB})
endif()

ExternalProject_Add(lv2_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/lv2
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/lv2_build
  CONFIGURE_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled --prefix=${EXTERNAL_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${FIND_PKGCONF_DIRS}
)

ExternalProject_Add(serd_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/serd
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/serd_build
  CONFIGURE_COMMAND
    ${MESON_ENV} ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled -Dtests=disabled --prefix=${EXTERNAL_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${FIND_PKGCONF_DIRS}
    COMMAND ${PYTHON_CMD} ${CMAKE_SOURCE_DIR}/scripts/replace_symlink_by_target.py ${SERD_LIB}
    COMMAND ${CMAKE_COMMAND} -E copy ${SERD_LIB} ${CMAKE_CURRENT_BINARY_DIR}
  DEPENDS lv2_proj
  BUILD_BYPRODUCTS ${SERD_OUTPUTS}
)

ExternalProject_Add(sord_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/sord
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/sord_build
  CONFIGURE_COMMAND
    ${MESON_ENV} ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled -Dtests=disabled --prefix=${EXTERNAL_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${PYTHON_CMD} ${CMAKE_SOURCE_DIR}/scripts/replace_symlink_by_target.py ${SORD_LIB}
    COMMAND ${CMAKE_COMMAND} -E copy ${SORD_LIB} ${CMAKE_CURRENT_BINARY_DIR}
  DEPENDS lv2_proj serd_proj
  BUILD_BYPRODUCTS ${SORD_OUTPUTS}
)

ExternalProject_Add(sratom_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/sratom
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/sratom_build
  CONFIGURE_COMMAND
    ${MESON_ENV} ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled -Dtests=disabled --prefix=${EXTERNAL_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${PYTHON_CMD} ${CMAKE_SOURCE_DIR}/scripts/replace_symlink_by_target.py ${SRATOM_LIB}
    COMMAND ${CMAKE_COMMAND} -E copy ${SRATOM_LIB} ${CMAKE_CURRENT_BINARY_DIR}
  DEPENDS lv2_proj serd_proj sord_proj
  BUILD_BYPRODUCTS ${SRATOM_OUTPUTS}
)

ExternalProject_Add(lilv_proj
  SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/lilv
  BINARY_DIR ${CMAKE_CURRENT_BINARY_DIR}/lilv_build
  CONFIGURE_COMMAND
    ${MESON_ENV} ${WITH_PKGCONF_CMD} ${PYTHON_CMD} -m mesonbuild.mesonmain setup -Ddocs=disabled -Dtests=disabled --prefix=${EXTERNAL_DEPS_PREFIX} <BINARY_DIR> <SOURCE_DIR>
  BUILD_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain compile
  INSTALL_COMMAND
    ${MESON_ENV} ${PYTHON_CMD} -m mesonbuild.mesonmain install
    COMMAND ${PYTHON_CMD} ${CMAKE_SOURCE_DIR}/scripts/replace_symlink_by_target.py ${LILV_LIB}
    COMMAND ${CMAKE_COMMAND} -E copy ${LILV_LIB} ${CMAKE_CURRENT_BINARY_DIR}
  DEPENDS lv2_proj serd_proj sord_proj sratom_proj
  BUILD_BYPRODUCTS ${LILV_OUTPUTS}
)

add_library(lilv SHARED IMPORTED)
add_library(serd SHARED IMPORTED)
add_library(sord SHARED IMPORTED)
add_library(sratom SHARED IMPORTED)
set_property(TARGET lilv PROPERTY IMPORTED_LOCATION ${LILV_LIB})
set_property(TARGET serd PROPERTY IMPORTED_LOCATION ${SERD_LIB})
set_property(TARGET sord PROPERTY IMPORTED_LOCATION ${SORD_LIB})
set_property(TARGET sratom PROPERTY IMPORTED_LOCATION ${SRATOM_LIB})
if(WIN32)
set_property(TARGET lilv PROPERTY IMPORTED_IMPLIB ${LILV_IMPLIB})
set_property(TARGET serd PROPERTY IMPORTED_IMPLIB ${SERD_IMPLIB})
set_property(TARGET sord PROPERTY IMPORTED_IMPLIB ${SORD_IMPLIB})
set_property(TARGET sratom PROPERTY IMPORTED_IMPLIB ${SRATOM_IMPLIB})
endif()
add_dependencies(lilv lilv_proj)
add_dependencies(serd serd_proj)
add_dependencies(sord sord_proj)
add_dependencies(sratom sratom_proj)

set(LILV_INCLUDE_DIRS ${CMAKE_SOURCE_DIR}/../third_party/lilv/include)
set(LILV_TARGETS lilv sord serd sratom)
set(LV2_INCLUDE_DIRS ${EXTERNAL_DEPS_PREFIX}/include)

install(FILES ${LILV_LIB} ${SERD_LIB} ${SORD_LIB} ${SRATOM_LIB}
    EXCLUDE_FROM_ALL
    COMPONENT package_install
    DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
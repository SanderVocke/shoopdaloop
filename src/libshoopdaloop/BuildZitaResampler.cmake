include(ExternalProject)

set(ZITA_SOURCE_DIR ${CMAKE_SOURCE_DIR}/../third_party/zita-resampler)
set(ZITA_BUILD_DIR ${CMAKE_CURRENT_BINARY_DIR}/zita-resampler-build)
set(ZITA_TOPLEVEL_INCLUDE_DIR ${ZITA_SOURCE_DIR}/source)
set(ZITA_BUILD_TARGETS zita-resampler)

if(WIN32)
set(ZITA_LIB ${ZITA_BUILD_DIR}/zita-resampler.lib)
else()
set(ZITA_LIB ${ZITA_BUILD_DIR}/libzita-resampler.a)
endif()
set(ZITA_OUTPUTS ${ZITA_LIB})

set(CMAKE_BUILD_TARGET_ARGS)
foreach(item ${ZITA_BUILD_TARGETS})
    list(APPEND CMAKE_BUILD_TARGET_ARGS --target ${item})
endforeach()

ExternalProject_Add(zita_proj
  BUILD_COMMAND cmake --build <BINARY_DIR> ${CMAKE_BUILD_TARGET_ARGS}
  INSTALL_COMMAND ""
  SOURCE_DIR ${ZITA_SOURCE_DIR}
  BINARY_DIR ${ZITA_BUILD_DIR}
  BUILD_BYPRODUCTS ${ZITA_OUTPUTS}
)

add_library(zita INTERFACE)
target_link_libraries(zita INTERFACE ${ZITA_LIB})
target_include_directories(zita INTERFACE ${ZITA_TOPLEVEL_INCLUDE_DIR})


if(WIN32)
  # Link to the dll stub
  find_library(PTHREAD_WIN32_LIB
               pthreadVC2.lib
               REQUIRED
               PATH_SUFFIXES lib/x64)
  target_link_libraries(zita INTERFACE ${PTHREAD_WIN32_LIB})

  # Find the DLL
  find_file(PTHREAD_WIN32_DLL
               pthreadVC2.dll
               REQUIRED
               PATH_SUFFIXES dll/x64)
  # Install it into our package
  install(FILES ${PTHREAD_WIN32_DLL}
    EXCLUDE_FROM_ALL
    COMPONENT package_install
    DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
endif()
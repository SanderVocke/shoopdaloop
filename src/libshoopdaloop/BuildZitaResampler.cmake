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

set(ZITA_CMAKE_ARGS "")
if (WIN32)
  if (DEFINED PTHREADS4W_PATH)
    set(ZITA_CMAKE_ARGS
        -DCMAKE_PREFIX_PATH=${PTHREADS4W_PATH}
        -DCMAKE_LIBRARY_PATH=${PTHREADS4W_PATH}
        -DCMAKE_CXX_FLAGS=-I${PTHREADS4W_PATH}
        -DCMAKE_C_FLAGS=-I${PTHREADS4W_PATH}
        )
  endif()
endif()

ExternalProject_Add(zita_proj
  BUILD_COMMAND cmake --build <BINARY_DIR> ${CMAKE_BUILD_TARGET_ARGS}
  INSTALL_COMMAND ""
  SOURCE_DIR ${ZITA_SOURCE_DIR}
  BINARY_DIR ${ZITA_BUILD_DIR}
  BUILD_BYPRODUCTS ${ZITA_OUTPUTS}
  CMAKE_ARGS ${ZITA_CMAKE_ARGS}
)

add_library(zita INTERFACE)
target_link_libraries(zita INTERFACE ${ZITA_LIB})
target_include_directories(zita INTERFACE ${ZITA_TOPLEVEL_INCLUDE_DIR})


if(WIN32)
  # Link to the dll stub
  find_library(PTHREAD_WIN32_LIB
               pthreadVC3.lib
               REQUIRED
               HINTS ${PTHREADS4W_PATH})
  target_link_libraries(zita INTERFACE ${PTHREAD_WIN32_LIB})

  # Find the DLL
  find_file(PTHREAD_WIN32_DLL
               pthreadVC3.dll
               REQUIRED
               HINTS ${PTHREADS4W_PATH})
  # # Install it into our package
  # install(FILES ${PTHREAD_WIN32_DLL}
  #   EXCLUDE_FROM_ALL
  #   COMPONENT package_install
  #   DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})
endif()
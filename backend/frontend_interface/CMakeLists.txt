# Backend lib
add_library(shoopdaloop_backend SHARED shoopdaloop_backend.cpp ../LoggingBackend.cpp)
target_link_libraries(shoopdaloop_backend PUBLIC
                      lv2_evbuf
                      ${BOOST_LIBRARIES}
                      ${JACK_LIBRARIES}
                      ${LV2_LIBRARIES}
                      ${LILV_LIBRARIES}
                      ${SPDLOG_LIBRARIES}
                      fmt::fmt)
target_include_directories(shoopdaloop_backend
                        PRIVATE
                        ${CMAKE_CURRENT_SOURCE_DIR}/..
                        ${CMAKE_SOURCE_DIR}/third_party/lv2_evbuf
                        ${CMAKE_SOURCE_DIR}/third_party/base64/include
                        ${JACK_INCLUDE_DIRS}
                        ${CMAKE_CURRENT_BINARY_DIR}
                        ${BOOST_INCLUDE_DIRS}
                        ${LV2_INCLUDE_DIRS}
                        ${LILV_INCLUDE_DIRS}
                        ${CMAKE_SOURCE_DIR}/third_party/lv2_external_ui
                        )

# Generate C bindings
add_custom_command(
  OUTPUT shoopdaloop_backend.py
  COMMAND ${RUN_CMD_PREFIX} ctypesgen --no-macro-warnings -lshoopdaloop_backend ${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop_backend.h ${CMAKE_CURRENT_SOURCE_DIR}/types.h -o shoopdaloop_backend.py
  DEPENDS ${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop_backend.h ${CMAKE_CURRENT_SOURCE_DIR}/shoopdaloop_backend.cpp
  COMMENT "Generating C bindings for back-end."
  )
add_custom_target(
  backend_c_bindings
  ALL
  DEPENDS shoopdaloop_backend.py
  )

# Generate javascript copy of enums
add_custom_command(
  OUTPUT types.js
  COMMAND ${RUN_CMD_PREFIX} python gen_js_enums.py ${CMAKE_CURRENT_BINARY_DIR}/types.js
  COMMENT "Generating Javascript types."
  DEPENDS backend_c_bindings shoopdaloop_backend
  WORKING_DIRECTORY ${CMAKE_SOURCE_DIR}/codegen
  )
add_custom_target(
  javascript_types
  ALL
  DEPENDS types.js
  )
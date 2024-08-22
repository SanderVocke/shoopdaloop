set(CRATE shoop_rs_frontend)

corrosion_import_crate(MANIFEST_PATH Cargo.toml CRATES ${CRATE})
set(CXXQT_EXPORT_DIR "${CMAKE_CURRENT_BINARY_DIR}/cxxqt")
corrosion_set_env_vars(${CRATE}
    "CXXQT_EXPORT_DIR=${CXXQT_EXPORT_DIR}"
    "QMAKE=${QMAKE}"
)

get_target_property(shoop_rs_frontend_SHARED shoop_rs_frontend INTERFACE_LINK_LIBRARIES)
install(FILES $<TARGET_PROPERTY:${shoop_rs_frontend_SHARED},IMPORTED_LOCATION>
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})


# Build Cargo test executable and include in the installation.
set(TEST_EXECUTABLE "${CMAKE_CURRENT_BINARY_DIR}/shoop_rs_frontend_testrunner${EXEC_SUFFIX}")
add_custom_command(
    OUTPUT ${TEST_EXECUTABLE}
    WORKING_DIRECTORY ${CMAKE_CURRENT_SOURCE_DIR}
    COMMAND cmake -E env CARGO_TARGET_DIR="${CMAKE_CURRENT_BINARY_DIR}/test_target" cargo build --tests
    COMMAND ${GLOB_CMD} executable do_copy ${TEST_EXECUTABLE} "${CMAKE_CURRENT_BINARY_DIR}/test_target/**/shoop_rs_frontend-*"
    COMMENT "Finding shoop_rs_frontend_testrunner."
)
add_custom_target(build_shoop_rs_frontend_testrunner ALL DEPENDS shoop_rs_frontend ${TEST_EXECUTABLE})
install(PROGRAMS ${TEST_EXECUTABLE}
        EXCLUDE_FROM_ALL
        COMPONENT package_install
        DESTINATION ${PY_BUILD_CMAKE_MODULE_NAME})

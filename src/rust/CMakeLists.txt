if(WIN32)
    if(CMAKE_CXX_COMPILER_ID STEQUAL "MSVC")
        set(Rust_TOOLCHAIN "stable-x86_64-pc-windows-msvc")
    else()
        set(Rust_TOOLCHAIN "stable-x86_64-pc-windows-gnu")
    endif()
endif()

add_subdirectory(shoop_rs)
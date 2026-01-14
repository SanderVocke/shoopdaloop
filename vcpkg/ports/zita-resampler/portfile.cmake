vcpkg_download_distfile(ARCHIVE
    URLS "https://kokkinizita.linuxaudio.org/linuxaudio/downloads/zita-resampler-1.11.2.tar.xz"
    FILENAME "zita-resampler-1.11.2.tar.xz"
    SHA512 1598c9ead4bf858d3a11677c9512932077e1d0b83588682eba402820936fa1cfc5fe1112abbecd945469b4ae2f7a6f59938a5fbb0fdd79de3b0a3a73703b03dd
)

vcpkg_extract_source_archive(
    SOURCE_PATH
    ARCHIVE "${ARCHIVE}"
)

file(WRITE "${SOURCE_PATH}/CMakeLists.txt" "
cmake_minimum_required(VERSION 3.15)
project(zita-resampler LANGUAGES CXX)

find_package(SndFile CONFIG REQUIRED)

if(WIN32)
    find_package(PThreads4W REQUIRED)
    find_package(getopt CONFIG REQUIRED)
else()
    find_package(Threads REQUIRED)
endif()

add_library(zita-resampler
    source/cresampler.cc
    source/resampler.cc
    source/vresampler.cc
    source/resampler-table.cc
)
target_include_directories(zita-resampler PUBLIC
    $<BUILD_INTERFACE:\${CMAKE_CURRENT_SOURCE_DIR}/source>
    $<INSTALL_INTERFACE:include>
)
target_compile_features(zita-resampler PUBLIC cxx_std_11)

if(BUILD_SHARED_LIBS)
    target_compile_definitions(zita-resampler PUBLIC ZITA_RESAMPLER_SHARED)
else()
    target_compile_definitions(zita-resampler PUBLIC ZITA_RESAMPLER_STATIC)
endif()

# Define VERSION for apps
add_compile_definitions(VERSION=\\\"1.11.2\\\")

# Apps
add_executable(zresample
    apps/zresample.cc
    apps/audiofile.cc
    apps/dither.cc
)

if(WIN32)
    target_link_libraries(zresample PRIVATE zita-resampler SndFile::sndfile PThreads4W::PThreads4W)
    if(TARGET getopt::getopt)
        target_link_libraries(zresample PRIVATE getopt::getopt)
    elseif(TARGET getopt::getopt_static)
        target_link_libraries(zresample PRIVATE getopt::getopt_static)
    elseif(TARGET getopt::getopt_shared)
        target_link_libraries(zresample PRIVATE getopt::getopt_shared)
    endif()
else()
    target_link_libraries(zresample PRIVATE zita-resampler SndFile::sndfile Threads::Threads)
endif()

target_include_directories(zresample PRIVATE apps)

add_executable(zretune
    apps/zretune.cc
    apps/audiofile.cc
    apps/dither.cc
)

if(WIN32)
    target_link_libraries(zretune PRIVATE zita-resampler SndFile::sndfile PThreads4W::PThreads4W)
    if(TARGET getopt::getopt)
        target_link_libraries(zretune PRIVATE getopt::getopt)
    elseif(TARGET getopt::getopt_static)
        target_link_libraries(zretune PRIVATE getopt::getopt_static)
    elseif(TARGET getopt::getopt_shared)
        target_link_libraries(zretune PRIVATE getopt::getopt_shared)
    endif()
else()
    target_link_libraries(zretune PRIVATE zita-resampler SndFile::sndfile Threads::Threads)
endif()

target_include_directories(zretune PRIVATE apps)


install(TARGETS zita-resampler zresample zretune
    EXPORT zita-resampler-config
    RUNTIME DESTINATION bin
    LIBRARY DESTINATION lib
    ARCHIVE DESTINATION lib
)

install(DIRECTORY source/zita-resampler DESTINATION include)

install(EXPORT zita-resampler-config
    NAMESPACE zita-resampler::
    DESTINATION share/zita-resampler
)
")

vcpkg_cmake_configure(
    SOURCE_PATH "${SOURCE_PATH}"
)

vcpkg_cmake_install()

vcpkg_copy_pdbs()

vcpkg_cmake_config_fixup()

file(INSTALL "${SOURCE_PATH}/COPYING" DESTINATION "${CURRENT_PACKAGES_DIR}/share/${PORT}" RENAME copyright)

# Move tools to the right place if needed, vcpkg usually handles tools/zita-resampler/
vcpkg_copy_tools(TOOL_NAMES zresample zretune AUTO_CLEAN)

file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/debug/include")
file(REMOVE_RECURSE "${CURRENT_PACKAGES_DIR}/debug/share")
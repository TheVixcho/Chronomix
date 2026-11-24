cmake_minimum_required(VERSION 3.22)
project(Chronomix VERSION 0.0.1 LANGUAGES C CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(EXISTS "${CMAKE_SOURCE_DIR}/third_party/JUCE/CMakeLists.txt")
    add_subdirectory(third_party/JUCE)
else()
    include(FetchContent)
    FetchContent_Declare(
        JUCE
        GIT_REPOSITORY https://github.com/juce-framework/JUCE.git
        GIT_TAG 9.0.6
        GIT_SHALLOW TRUE
    )
    FetchContent_MakeAvailable(JUCE)
endif()

if(CMAKE_BUILD_TYPE STREQUAL "Release" OR CMAKE_BUILD_TYPE STREQUAL "RelWithDebInfo")
    set(CARGO_PROFILE_FLAG "--release")
    set(CARGO_TARGET_SUBDIR "release")
else()
    set(CARGO_PROFILE_FLAG "")
    set(CARGO_TARGET_SUBDIR "debug")
endif()

if(MSVC)
    set(RUST_STATIC_LIB "${CMAKE_SOURCE_DIR}/target/${CARGO_TARGET_SUBDIR}/chronomix_ffi.lib")
else()
    set(RUST_STATIC_LIB "${CMAKE_SOURCE_DIR}/target/${CARGO_TARGET_SUBDIR}/libchronomix_ffi.a")
endif()

add_custom_target(chronomix_rust_core ALL
    COMMAND cargo build --package chronomix-ffi ${CARGO_PROFILE_FLAG}
    WORKING_DIRECTORY "${CMAKE_SOURCE_DIR}"
    BYPRODUCTS "${RUST_STATIC_LIB}"
    USES_TERMINAL
)

juce_add_gui_app(chronomix
    PRODUCT_NAME "Chronomix"
    COMPANY_NAME "TheVixcho"
    BUNDLE_ID "io.github.thevixcho.chronomix"
    VERSION 0.0.1
)

target_sources(chronomix PRIVATE
    juce-app/src/Main.cpp
    juce-app/src/MainComponent.cpp
    juce-app/src/MixerPanel.cpp
    juce-app/src/MidiPanel.cpp
    juce-app/src/DjPanel.cpp
    juce-app/src/LibraryPanel.cpp
)

target_include_directories(chronomix PRIVATE
    "${CMAKE_SOURCE_DIR}/crates/chronomix-ffi/include"
    "${CMAKE_SOURCE_DIR}/juce-app/src"
)

target_compile_definitions(chronomix PRIVATE
    JUCE_WEB_BROWSER=0
    JUCE_USE_CURL=0
    JUCE_USE_MP3AUDIOFORMAT=1
    JUCE_USE_FLAC=1
    JUCE_USE_OGGVORBIS=1
    JUCE_APPLICATION_NAME_STRING="$<TARGET_PROPERTY:chronomix,JUCE_PRODUCT_NAME>"
    JUCE_APPLICATION_VERSION_STRING="$<TARGET_PROPERTY:chronomix,JUCE_VERSION>"
)

add_dependencies(chronomix chronomix_rust_core)

target_link_libraries(chronomix PRIVATE
    "${RUST_STATIC_LIB}"
    juce::juce_gui_extra
    juce::juce_gui_basics
    juce::juce_audio_formats
    juce::juce_graphics
    juce::juce_events
    juce::juce_data_structures
    juce::juce_core
    juce::juce_recommended_config_flags
    juce::juce_recommended_warning_flags
)

if(UNIX AND NOT APPLE)
    find_package(PkgConfig REQUIRED)
    pkg_check_modules(ALSA REQUIRED alsa)
    pkg_check_modules(PIPEWIRE REQUIRED libpipewire-0.3)
    pkg_check_modules(PULSE REQUIRED libpulse)
    pkg_check_modules(XCB REQUIRED xcb)
    pkg_check_modules(X11 REQUIRED x11)
    target_link_libraries(chronomix PRIVATE
        ${ALSA_LIBRARIES}
        ${PIPEWIRE_LIBRARIES}
        ${PULSE_LIBRARIES}
        ${XCB_LIBRARIES}
        ${X11_LIBRARIES}
        dl
        pthread
        m
    )
elseif(APPLE)
    target_link_libraries(chronomix PRIVATE
        "-framework CoreAudio"
        "-framework AudioToolbox"
        "-framework CoreFoundation"
        "-framework CoreMIDI"
        "-framework Security"
    )
elseif(WIN32)
    target_link_libraries(chronomix PRIVATE
        ws2_32
        userenv
        bcrypt
        ole32
        oleaut32
        winmm
        advapi32
        ntdll
        shell32
        shlwapi
    )
endif()

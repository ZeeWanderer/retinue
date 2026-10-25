cmake_minimum_required(VERSION 3.28 FATAL_ERROR)

file(STRINGS "${CMAKE_CURRENT_SOURCE_DIR}/VERSION" WFCLI_VERSION LIMIT_COUNT 1)
file(READ "${CMAKE_CURRENT_SOURCE_DIR}/apps/wfgui/qt/vcpkg.json" WFCLI_VCPKG_MANIFEST)
string(JSON WFCLI_VCPKG_VERSION GET "${WFCLI_VCPKG_MANIFEST}" version-string)
if(NOT WFCLI_VCPKG_VERSION STREQUAL WFCLI_VERSION)
  message(FATAL_ERROR "apps/wfgui/qt/vcpkg.json version must match root VERSION")
endif()
set(WFCLI_LOCAL_PROTOCOL_FILE
  "${CMAKE_CURRENT_SOURCE_DIR}/apps/retinue_daemon/src/runtime/retinue_local_protocol.erl")
set_property(DIRECTORY APPEND PROPERTY CMAKE_CONFIGURE_DEPENDS
  "${CMAKE_CURRENT_SOURCE_DIR}/VERSION" "${WFCLI_LOCAL_PROTOCOL_FILE}")
function(retinue_protocol_define NAME OUTPUT)
  file(STRINGS "${WFCLI_LOCAL_PROTOCOL_FILE}" LINE
    REGEX "^-define\\(${NAME}," LIMIT_COUNT 1)
  string(REGEX MATCH "[0-9]+" VALUE "${LINE}")
  if(NOT VALUE)
    message(FATAL_ERROR "Could not read Retinue protocol define ${NAME}")
  endif()
  set(${OUTPUT} "${VALUE}" PARENT_SCOPE)
endfunction()

retinue_protocol_define(ENVELOPE_VERSION WFCLI_LOCAL_ENVELOPE)
foreach(INTERFACE DATASETS PLAYER WORLDSTATE NOTIFICATIONS MARKET OVERFRAME
                  RELICS ASSETS BUILDS DIAGNOSTICS)
  retinue_protocol_define("INTERFACE_${INTERFACE}" "WFCLI_INTERFACE_${INTERFACE}")
endforeach()

include(cmake/gui-toolchain.cmake)

find_program(WFCLI_SCCACHE NAMES sccache)
if(WFCLI_SCCACHE)
  execute_process(
    COMMAND "${WFCLI_SCCACHE}" --start-server
    OUTPUT_QUIET
    ERROR_QUIET
  )
endif()

include(cmake/gui-vcpkg.cmake)

project(retinue VERSION "${WFCLI_VERSION}" LANGUAGES CXX)

set(WFCLI_CPU_BASELINE "" CACHE STRING "Portable production CPU baseline")
if(WFCLI_CPU_BASELINE)
  add_compile_options("-march=${WFCLI_CPU_BASELINE}")
  add_link_options("-march=${WFCLI_CPU_BASELINE}")
endif()

if(WFCLI_LLVM_ROOT)
  list(APPEND CMAKE_BUILD_RPATH "${WFCLI_LLVM_ROOT}/lib")
endif()

include(CTest)
if(BUILD_TESTING)
  add_test(NAME gui_toolchain
    COMMAND "${CMAKE_COMMAND}"
      "-DTEST_DIR=${CMAKE_CURRENT_BINARY_DIR}/toolchain-test"
      -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/tests/gui-toolchain.cmake")
  add_test(NAME gui_vcpkg
    COMMAND "${CMAKE_COMMAND}"
      "-DTEST_DIR=${CMAKE_CURRENT_BINARY_DIR}/vcpkg-test"
      -P "${CMAKE_CURRENT_SOURCE_DIR}/cmake/tests/gui-vcpkg.cmake")
endif()
add_subdirectory(apps/wfgui/qt)

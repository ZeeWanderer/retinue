cmake_minimum_required(VERSION 3.24 FATAL_ERROR)

find_program(RETINUE_SCCACHE NAMES sccache)
if(RETINUE_SCCACHE)
  execute_process(
    COMMAND "${RETINUE_SCCACHE}" --start-server
    OUTPUT_QUIET
    ERROR_QUIET
  )
  set(CMAKE_C_COMPILER_LAUNCHER "${RETINUE_SCCACHE}" CACHE FILEPATH "" FORCE)
  set(CMAKE_CXX_COMPILER_LAUNCHER "${RETINUE_SCCACHE}" CACHE FILEPATH "" FORCE)
endif()

project(retinue_overlay_native LANGUAGES C CXX)

set(BLEND2D_DIR "" CACHE PATH "Blend2D source directory")
set(ASMJIT_DIR "" CACHE PATH "AsmJit source directory")
set(RETINUE_CPU_BASELINE "" CACHE STRING "Portable production CPU baseline")

if(NOT EXISTS "${BLEND2D_DIR}/CMakeLists.txt")
  message(FATAL_ERROR "BLEND2D_DIR does not contain Blend2D")
endif()
if(NOT EXISTS "${ASMJIT_DIR}/CMakeLists.txt")
  message(FATAL_ERROR "ASMJIT_DIR does not contain AsmJit")
endif()

set(CMAKE_ARCHIVE_OUTPUT_DIRECTORY "${CMAKE_BINARY_DIR}/lib")
set(CMAKE_POSITION_INDEPENDENT_CODE ON)
set(BLEND2D_STATIC ON CACHE BOOL "" FORCE)
set(BLEND2D_TEST OFF CACHE BOOL "" FORCE)
set(BLEND2D_DEMOS OFF CACHE BOOL "" FORCE)
set(BLEND2D_NO_INSTALL ON CACHE BOOL "" FORCE)

if(RETINUE_CPU_BASELINE)
  add_compile_options("-march=${RETINUE_CPU_BASELINE}")
endif()

add_subdirectory("${BLEND2D_DIR}" blend2d)

add_library(retinue_blend2d_bridge STATIC blend2d_bridge.c)
target_compile_features(retinue_blend2d_bridge PRIVATE c_std_11)
target_compile_definitions(retinue_blend2d_bridge PRIVATE BL_STATIC)
target_include_directories(
  retinue_blend2d_bridge
  PRIVATE "${BLEND2D_DIR}/blend2d"
)
target_link_libraries(retinue_blend2d_bridge PRIVATE blend2d::blend2d)

find_package(Qt6 REQUIRED COMPONENTS Network WebSockets Widgets)

qt_standard_project_setup()

qt_add_library(wfgui_core STATIC
  src/activity_data.cpp
  src/activity_data.h
  src/asset_ref.cpp
  src/asset_ref.h
  src/app_controller.cpp
  src/app_controller.h
  src/animated_progress_bar.cpp
  src/animated_progress_bar.h
  src/arcane_card_widget.cpp
  src/arcane_card_widget.h
  src/build_equipment_model.cpp
  src/build_equipment_model.h
  src/build_group_model.cpp
  src/build_group_model.h
  src/build_source_model.cpp
  src/build_source_model.h
  src/build_topology_widget.cpp
  src/build_topology_widget.h
  src/build_plan_widget.cpp
  src/build_plan_widget.h
  src/compact_search.cpp
  src/compact_search.h
  src/daemon_client.cpp
  src/daemon_client.h
  src/derivative_cache.cpp
  src/derivative_cache.h
  src/image_cache.cpp
  src/image_cache.h
  src/thumbnail_widget.cpp
  src/thumbnail_widget.h
  src/inventory_card_layout.cpp
  src/inventory_card_layout.h
  src/market_order_card.cpp
  src/market_order_card.h
  src/market_item_dialog.cpp
  src/market_item_dialog.h
  src/market_spin_box.cpp
  src/market_spin_box.h
  src/mod_card_widget.cpp
  src/mod_card_widget.h
  src/overframe_login.cpp
  src/overframe_login.h
  src/market_rail_widget.cpp
  src/market_rail_widget.h
  src/player_item_grid_widget.cpp
  src/player_item_grid_widget.h
  src/player_item_model.cpp
  src/player_item_model.h
  src/display_scale.cpp
  src/display_scale.h
  src/relic_card_layout.cpp
  src/relic_card_layout.h
  src/relic_grid_widget.cpp
  src/relic_grid_widget.h
  src/relic_model.cpp
  src/relic_model.h
  src/settings_widget.cpp
  src/settings_widget.h
  src/wfgui_paths.cpp
  src/wfgui_paths.h
  src/display_metrics.h
  src/tooltip.cpp
  src/tooltip.h
  src/widget_capture.cpp
  src/widget_capture.h
)

target_sources(wfgui_core
  PUBLIC
    FILE_SET CXX_MODULES
    FILES src/relic_data.cppm
)

qt_add_executable(wfgui
  src/activity_rail_widget.cpp
  src/activity_rail_widget.h
  src/build_equipment_widget.cpp
  src/build_equipment_widget.h
  src/build_discover_widget.cpp
  src/build_discover_widget.h
  src/build_groups_widget.cpp
  src/build_groups_widget.h
  src/build_planner_widget.cpp
  src/build_planner_widget.h
  src/foundry_widget.cpp
  src/foundry_widget.h
  src/inventory_widget.cpp
  src/inventory_widget.h
  src/main.cpp
  src/main_window.cpp
  src/main_window.h
  src/market_widget.cpp
  src/market_widget.h
  src/mastery_planner_widget.cpp
  src/mastery_planner_widget.h
  src/player_identity_widget.cpp
  src/player_identity_widget.h
  src/relic_planner_widget.cpp
  src/relic_planner_widget.h
  src/title_bar_widget.cpp
  src/title_bar_widget.h
  src/style_loader.cpp
  src/style_loader.h
)

set_target_properties(wfgui PROPERTIES OUTPUT_NAME "wfgui-qt")

set(WFGUI_RESOURCES
    resources/categories/all.png
    resources/categories/arcanes.png
    resources/categories/arch.png
    resources/categories/companion.png
    resources/categories/melee.png
    resources/categories/misc.png
    resources/categories/mods.png
    resources/categories/modular.png
    resources/categories/primary.png
    resources/categories/relics.png
    resources/categories/secondary.png
    resources/categories/sets.png
    resources/categories/warframe.png
    resources/activity/axi.png
    resources/activity/aya.png
    resources/activity/cambion.png
    resources/activity/earth.png
    resources/activity/lith.png
    resources/activity/meso.png
    resources/activity/neo.png
    resources/activity/omnia.png
    resources/activity/requiem.png
    resources/activity/resurgence.png
    resources/activity/vallis.png
    resources/app/wfgui-512.png
    resources/ui/all.png
    resources/ui/chevron-down.png
    resources/ui/favorite.png
    resources/ui/favorite_active.png
    resources/ui/filter.png
    resources/ui/fissure.png
    resources/ui/mastery_easy.png
    resources/ui/mastery_rank.png
    resources/ui/mastery_relics.png
    resources/ui/market.png
    resources/ui/nav_foundry.png
    resources/ui/nav_inventory.png
    resources/ui/nav_mastery.png
    resources/ui/nav_relic.png
    resources/ui/nav_builds.png
    resources/ui/normal.png
    resources/ui/notification.png
    resources/ui/panel-left.png
    resources/ui/panel-right.png
    resources/ui/pending.png
    resources/ui/refresh.png
    resources/ui/search.png
    resources/ui/settings.png
    resources/ui/steel.png
    resources/ui/summary_game.png
    resources/ui/summary_intrinsics.png
    resources/ui/summary_star.png
    resources/ui/world.png
    resources/ui/window-close.png
    resources/ui/window-maximize.png
    resources/ui/window-minimize.png
    resources/ui/window-restore.png
    resources/ui/zoomin.png
    resources/ui/zoomout.png
    resources/market/wfgui-boxes.png
    resources/market/wfgui-check.png
    resources/market/wfgui-eye-off.png
    resources/market/wfgui-eye.png
    resources/market/wfgui-pencil.png
    resources/market/wfgui-plus.png
    resources/market/wfgui-search.png
    resources/market/wfgui-trash.png
    resources/polarities/aura.png
    resources/polarities/madurai.png
    resources/polarities/naramon.png
    resources/polarities/penjaga.png
    resources/polarities/umbra.png
    resources/polarities/unairu.png
    resources/polarities/universal.png
    resources/polarities/vazarin.png
    resources/polarities/zenurik.png
    resources/arcane-frames/common.png
    resources/arcane-frames/empty.png
    resources/arcane-frames/legendary.png
    resources/arcane-frames/rare.png
    resources/arcane-frames/uncommon.png
    resources/mod-frames/AmalgamBackground.png
    resources/mod-frames/AmalgamFrameBottom.png
    resources/mod-frames/AmalgamFrameTop.png
    resources/mod-frames/BronzeBackground.png
    resources/mod-frames/BronzeFrameBottom.png
    resources/mod-frames/BronzeFrameTop.png
    resources/mod-frames/BronzeLowerTab.png
    resources/mod-frames/BronzeSideLight.png
    resources/mod-frames/GalvanizedFrameBottom.png
    resources/mod-frames/GalvanizedFrameTop.png
    resources/mod-frames/GoldBackground.png
    resources/mod-frames/GoldFrameBottom.png
    resources/mod-frames/GoldFrameTop.png
    resources/mod-frames/GoldLowerTab.png
    resources/mod-frames/GoldSideLight.png
    resources/mod-frames/LegendaryBackground.png
    resources/mod-frames/LegendaryFrameBottom.png
    resources/mod-frames/LegendaryFrameTop.png
    resources/mod-frames/LegendaryLowerTab.png
    resources/mod-frames/LegendarySideLight.png
    resources/mod-frames/OmegaFrameBottom.png
    resources/mod-frames/OmegaFrameTop.png
    resources/mod-frames/OmegaSideLight.png
    resources/mod-frames/RivenLowerTab.png
    resources/mod-frames/SilverBackground.png
    resources/mod-frames/SilverFrameBottom.png
    resources/mod-frames/SilverFrameTop.png
    resources/mod-frames/SilverLowerTab.png
    resources/mod-frames/SilverSideLight.png
    resources/mod-slots/aura.png
    resources/mod-slots/empty.png
    resources/mod-slots/exilus.png
    resources/mod-slots/stance.png
    resources/styles/activity.qss
    resources/styles/content.qss
    resources/styles/controls.qss
    resources/styles/foundation.qss
    resources/styles/market.qss
)
foreach(icon helminth mastered owned)
  set(source "${PROJECT_SOURCE_DIR}/assets/warframe/ui/${icon}.png")
  set_source_files_properties("${source}" PROPERTIES QT_RESOURCE_ALIAS "resources/ui/${icon}.png")
  list(APPEND WFGUI_RESOURCES "${source}")
endforeach()
qt_add_resources(wfgui "wfgui_resources" PREFIX "/" FILES ${WFGUI_RESOURCES})

set(WFGUI_FONT_DIR "${PROJECT_SOURCE_DIR}/assets/fonts/roboto")
set(WFGUI_FONTS
    "${WFGUI_FONT_DIR}/Roboto-Bold.ttf"
    "${WFGUI_FONT_DIR}/Roboto-Light.ttf"
    "${WFGUI_FONT_DIR}/Roboto-Medium.ttf"
    "${WFGUI_FONT_DIR}/Roboto-Regular.ttf"
)
qt_add_resources(wfgui "wfgui_fonts"
  PREFIX "/assets"
  BASE "${WFGUI_FONT_DIR}"
  FILES ${WFGUI_FONTS}
)

set(WFGUI_SHARED_ASSETS
    ../../wfcompanion/assets/ducats.png
    ../../wfcompanion/assets/forma.png
    ../../wfcompanion/assets/platinum.png
    ../../wfcompanion/assets/unvaulted.png
    ../../wfcompanion/assets/vaulted.png
)
qt_add_resources(wfgui "wfgui_shared_assets"
  PREFIX "/assets"
  BASE "${CMAKE_CURRENT_SOURCE_DIR}/../../wfcompanion/assets"
  FILES ${WFGUI_SHARED_ASSETS}
)

target_compile_features(wfgui_core PUBLIC cxx_std_23)
set_target_properties(wfgui_core PROPERTIES CXX_SCAN_FOR_MODULES ON)
target_compile_definitions(wfgui_core PUBLIC
  WFCLI_LOCAL_ENVELOPE=${WFCLI_LOCAL_ENVELOPE}
  WFCLI_INTERFACE_DATASETS=${WFCLI_INTERFACE_DATASETS}
  WFCLI_INTERFACE_PLAYER=${WFCLI_INTERFACE_PLAYER}
  WFCLI_INTERFACE_WORLDSTATE=${WFCLI_INTERFACE_WORLDSTATE}
  WFCLI_INTERFACE_NOTIFICATIONS=${WFCLI_INTERFACE_NOTIFICATIONS}
  WFCLI_INTERFACE_MARKET=${WFCLI_INTERFACE_MARKET}
  WFCLI_INTERFACE_OVERFRAME=${WFCLI_INTERFACE_OVERFRAME}
  WFCLI_INTERFACE_RELICS=${WFCLI_INTERFACE_RELICS}
  WFCLI_INTERFACE_ASSETS=${WFCLI_INTERFACE_ASSETS}
  WFCLI_INTERFACE_BUILDS=${WFCLI_INTERFACE_BUILDS}
  WFCLI_INTERFACE_DIAGNOSTICS=${WFCLI_INTERFACE_DIAGNOSTICS}
  WFCLI_VERSION="${PROJECT_VERSION}"
)
target_include_directories(wfgui_core PUBLIC src)
target_link_libraries(wfgui_core PUBLIC Qt6::Network Qt6::WebSockets Qt6::Widgets)
target_link_libraries(wfgui PRIVATE Qt6::Widgets wfgui_core)
set_target_properties(wfgui PROPERTIES
  INSTALL_RPATH "$ORIGIN:$ORIGIN/../lib"
)

qt_add_executable(wfgui_icon_optics
  ../../../tools/icon-optics/main.cpp
)
set_target_properties(wfgui_icon_optics PROPERTIES
  OUTPUT_NAME "wfgui-icon-optics"
)
target_compile_features(wfgui_icon_optics PRIVATE cxx_std_23)
target_compile_definitions(wfgui_icon_optics PRIVATE
  WFCLI_SOURCE_DIR="${PROJECT_SOURCE_DIR}"
)
target_link_libraries(wfgui_icon_optics PRIVATE Qt6::Gui)

install(TARGETS wfgui RUNTIME DESTINATION bin)
install(FILES resources/qt.conf DESTINATION bin)
install(FILES resources/Lucide-LICENSE.txt DESTINATION share/licenses/wfgui)
install(FILES "${WFGUI_FONT_DIR}/Roboto-LICENSE.txt" DESTINATION share/licenses/fonts)
string(REGEX MATCH "^[0-9]+" llvm_major "${CMAKE_CXX_COMPILER_VERSION}")
find_file(WFCLI_LLVM_LICENSE_FILE NAMES LICENSE.TXT copyright
  PATHS "${WFCLI_LLVM_ROOT}"
        "/usr/share/doc/libc++-${llvm_major}-dev"
        "/usr/share/doc/libc++-dev"
  NO_DEFAULT_PATH REQUIRED)
install(FILES "${WFCLI_LLVM_LICENSE_FILE}"
  DESTINATION share/licenses/wfgui
  RENAME LLVM-LICENSE.txt
)
foreach(runtime c++ c++abi unwind)
  set(runtime_soname "lib${runtime}.so.1")
  file(REAL_PATH
    "${WFCLI_LLVM_ROOT}/lib/${runtime_soname}"
    runtime_file
  )
  install(FILES "${runtime_file}"
    DESTINATION lib
    RENAME "${runtime_soname}"
  )
  install(CODE
    "file(RPATH_REMOVE FILE \"\$ENV{DESTDIR}\${CMAKE_INSTALL_PREFIX}/lib/${runtime_soname}\")"
  )
endforeach()
install(FILES resources/app/wfgui.svg
  DESTINATION share/retinue/games/warframe/icons
)
foreach(icon_size 16 24 32 48 64 128 256 512)
  install(FILES resources/app/wfgui-${icon_size}.png
    DESTINATION share/retinue/games/warframe/icons
  )
endforeach()
install(
  DIRECTORY "${VCPKG_INSTALLED_DIR}/${VCPKG_TARGET_TRIPLET}/lib/"
  DESTINATION lib
  CONFIGURATIONS Release RelWithDebInfo MinSizeRel
  FILES_MATCHING PATTERN "*.so" PATTERN "*.so.*"
)
install(
  DIRECTORY "${VCPKG_INSTALLED_DIR}/${VCPKG_TARGET_TRIPLET}/debug/lib/"
  DESTINATION lib
  CONFIGURATIONS Debug
  FILES_MATCHING PATTERN "*.so" PATTERN "*.so.*"
)
install(
  DIRECTORY "${VCPKG_INSTALLED_DIR}/${VCPKG_TARGET_TRIPLET}/Qt6/plugins/"
  DESTINATION Qt6/plugins
  CONFIGURATIONS Release RelWithDebInfo MinSizeRel
)
install(
  DIRECTORY "${VCPKG_INSTALLED_DIR}/${VCPKG_TARGET_TRIPLET}/debug/Qt6/plugins/"
  DESTINATION Qt6/plugins
  CONFIGURATIONS Debug
)

if(CMAKE_CXX_COMPILER_ID MATCHES "Clang|GNU")
  target_compile_options(wfgui_core PRIVATE -Wall -Wextra -Wpedantic)
  target_compile_options(wfgui_icon_optics PRIVATE -Wall -Wextra -Wpedantic)
endif()

if(BUILD_TESTING)
  find_package(Qt6 REQUIRED COMPONENTS Test)
  qt_add_executable(wfgui_paths_tests tests/paths_test.cpp src/wfgui_paths.cpp src/display_scale.cpp)
  target_include_directories(wfgui_paths_tests PRIVATE src)
  target_link_libraries(wfgui_paths_tests PRIVATE Qt6::Core Qt6::Test)
  add_test(NAME wfgui-paths COMMAND wfgui_paths_tests)
  qt_add_executable(wfgui_market_tests
    src/market_widget.cpp
    src/market_widget.h
    tests/market_widget_test.cpp
  )
  target_link_libraries(wfgui_market_tests PRIVATE Qt6::Test wfgui_core)
  add_test(NAME wfgui-market COMMAND wfgui_market_tests)
  set_tests_properties(wfgui-market PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
  qt_add_executable(wfgui_io_tests tests/daemon_client_io_test.cpp)
  target_link_libraries(wfgui_io_tests PRIVATE Qt6::Test wfgui_core)
  add_test(NAME wfgui-io COMMAND wfgui_io_tests)
  set_tests_properties(wfgui-io PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen")
  qt_add_executable(wfgui_tests
    src/activity_rail_widget.cpp
    src/activity_rail_widget.h
    src/relic_planner_widget.cpp
    src/relic_planner_widget.h
    tests/relic_model_test.cpp
  )
  qt_add_resources(wfgui_tests "wfgui_test_resources"
    PREFIX "/"
    FILES
      resources/activity/requiem.png
  )
  target_link_libraries(wfgui_tests PRIVATE Qt6::Test wfgui_core)
  target_compile_definitions(wfgui_tests PRIVATE
    WFGUI_SOURCE_DIR="${CMAKE_CURRENT_SOURCE_DIR}"
  )
  add_test(NAME wfgui-models COMMAND wfgui_tests)
  set_tests_properties(wfgui-models PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
  )
  foreach(scale 1.25 2)
    add_test(NAME wfgui-thumbnail-${scale}
      COMMAND wfgui_tests thumbnailWidgetKeepsTintAndSurfaceAtDpr)
    set_tests_properties(wfgui-thumbnail-${scale} PROPERTIES
      ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_SCALE_FACTOR=${scale}")
  endforeach()
  qt_add_executable(wfgui_build_equipment_tests
    tests/build_equipment_model_test.cpp
  )
  target_link_libraries(wfgui_build_equipment_tests PRIVATE Qt6::Test wfgui_core)
  add_test(NAME wfgui-build-equipment COMMAND wfgui_build_equipment_tests)
  set_tests_properties(wfgui-build-equipment PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
  )
  qt_add_executable(wfgui_build_equipment_widget_tests
    src/build_equipment_widget.cpp
    src/build_equipment_widget.h
    tests/build_equipment_widget_test.cpp
  )
  qt_add_resources(wfgui_build_equipment_widget_tests
    "wfgui_build_equipment_widget_resources"
    PREFIX "/"
    FILES
      resources/ui/panel-left.png
      resources/ui/panel-right.png
  )
  target_link_libraries(wfgui_build_equipment_widget_tests PRIVATE
    Qt6::Test
    wfgui_core
  )
  add_test(NAME wfgui-build-equipment-widget
    COMMAND wfgui_build_equipment_widget_tests)
  set_tests_properties(wfgui-build-equipment-widget PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
  )
  qt_add_executable(wfgui_build_source_tests
    tests/build_source_model_test.cpp
  )
  target_link_libraries(wfgui_build_source_tests PRIVATE Qt6::Test wfgui_core)
  add_test(NAME wfgui-build-source COMMAND wfgui_build_source_tests)
  set_tests_properties(wfgui-build-source PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
  )
  qt_add_executable(wfgui_build_discover_widget_tests
    src/build_discover_widget.cpp
    src/build_discover_widget.h
    tests/build_discover_widget_test.cpp
  )
  target_link_libraries(wfgui_build_discover_widget_tests PRIVATE
    Qt6::Test
    wfgui_core
  )
  add_test(NAME wfgui-build-discover-widget
    COMMAND wfgui_build_discover_widget_tests)
  set_tests_properties(wfgui-build-discover-widget PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
  )
  qt_add_executable(wfgui_build_group_tests
    tests/build_group_model_test.cpp
  )
  target_link_libraries(wfgui_build_group_tests PRIVATE Qt6::Test wfgui_core)
  add_test(NAME wfgui-build-groups COMMAND wfgui_build_group_tests)
  set_tests_properties(wfgui-build-groups PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
  )
  qt_add_executable(wfgui_build_groups_widget_tests
    src/build_groups_widget.cpp
    src/build_groups_widget.h
    tests/build_groups_widget_test.cpp
    src/style_loader.cpp
  )
  qt_add_resources(wfgui_build_groups_widget_tests
    "wfgui_build_groups_widget_resources"
    PREFIX "/"
    FILES ${WFGUI_RESOURCES}
  )
  qt_add_resources(wfgui_build_groups_widget_tests "wfgui_build_groups_assets"
    PREFIX "/assets"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}/../../wfcompanion/assets"
    FILES ${WFGUI_SHARED_ASSETS}
  )
  qt_add_resources(wfgui_build_groups_widget_tests "wfgui_build_groups_fonts"
    PREFIX "/assets"
    BASE "${WFGUI_FONT_DIR}"
    FILES ${WFGUI_FONTS}
  )
  target_link_libraries(wfgui_build_groups_widget_tests PRIVATE
    Qt6::Test
    wfgui_core
  )
  add_test(NAME wfgui-build-groups-widget
    COMMAND wfgui_build_groups_widget_tests)
  set_tests_properties(wfgui-build-groups-widget PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
  )
  qt_add_executable(wfgui_build_topology_tests
    tests/build_topology_widget_test.cpp
  )
  qt_add_resources(wfgui_build_topology_tests "wfgui_build_topology_resources"
    PREFIX "/"
    FILES
      resources/mod-frames/AmalgamBackground.png
      resources/mod-frames/AmalgamFrameBottom.png
      resources/mod-frames/AmalgamFrameTop.png
      resources/mod-frames/BronzeBackground.png
      resources/mod-frames/BronzeFrameBottom.png
      resources/mod-frames/BronzeFrameTop.png
      resources/mod-frames/BronzeLowerTab.png
      resources/mod-frames/BronzeSideLight.png
      resources/mod-frames/GalvanizedFrameBottom.png
      resources/mod-frames/GalvanizedFrameTop.png
      resources/mod-frames/GoldBackground.png
      resources/mod-frames/GoldFrameBottom.png
      resources/mod-frames/GoldFrameTop.png
      resources/mod-frames/GoldLowerTab.png
      resources/mod-frames/GoldSideLight.png
      resources/mod-frames/LegendaryBackground.png
      resources/mod-frames/LegendaryFrameBottom.png
      resources/mod-frames/LegendaryFrameTop.png
      resources/mod-frames/LegendaryLowerTab.png
      resources/mod-frames/LegendarySideLight.png
      resources/mod-frames/OmegaFrameBottom.png
      resources/mod-frames/OmegaFrameTop.png
      resources/mod-frames/OmegaSideLight.png
      resources/mod-frames/RivenLowerTab.png
      resources/mod-frames/SilverBackground.png
      resources/mod-frames/SilverFrameBottom.png
      resources/mod-frames/SilverFrameTop.png
      resources/mod-frames/SilverLowerTab.png
      resources/mod-frames/SilverSideLight.png
      resources/mod-slots/aura.png
      resources/mod-slots/empty.png
      resources/mod-slots/exilus.png
      resources/mod-slots/stance.png
      resources/arcane-frames/common.png
      resources/arcane-frames/empty.png
      resources/arcane-frames/legendary.png
      resources/arcane-frames/rare.png
      resources/arcane-frames/uncommon.png
      resources/polarities/aura.png
      resources/polarities/madurai.png
      resources/polarities/naramon.png
      resources/polarities/penjaga.png
      resources/polarities/umbra.png
      resources/polarities/unairu.png
      resources/polarities/universal.png
      resources/polarities/vazarin.png
      resources/polarities/zenurik.png
  )
  qt_add_resources(wfgui_build_topology_tests "wfgui_build_topology_assets"
    PREFIX "/assets"
    BASE "${CMAKE_CURRENT_SOURCE_DIR}/../../wfcompanion/assets"
    FILES
      ../../wfcompanion/assets/forma.png
  )
  target_link_libraries(wfgui_build_topology_tests PRIVATE Qt6::Test wfgui_core)
  add_test(NAME wfgui-build-topology COMMAND wfgui_build_topology_tests)
  set_tests_properties(wfgui-build-topology PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
  )
  foreach(scale IN ITEMS 1.25 2)
    add_test(NAME wfgui-mod-artwork-${scale}
      COMMAND wfgui_build_topology_tests preservesModArtworkAtHighDpi)
    set_tests_properties(wfgui-mod-artwork-${scale} PROPERTIES
      ENVIRONMENT "QT_QPA_PLATFORM=offscreen;QT_SCALE_FACTOR=${scale}"
    )
  endforeach()
  add_test(
    NAME wfgui-icon-optics
    COMMAND wfgui_icon_optics
      --config "${PROJECT_SOURCE_DIR}/tools/icon-optics/foundry.json"
      --output-dir "${CMAKE_CURRENT_BINARY_DIR}/icon-optics-test"
      --ui-scale 1.25
  )
  set_tests_properties(wfgui-icon-optics PROPERTIES
    ENVIRONMENT "QT_QPA_PLATFORM=offscreen"
  )
endif()

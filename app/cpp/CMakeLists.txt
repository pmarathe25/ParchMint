qt_add_library(parchmint_adapters STATIC)
qt_add_qml_module(parchmint_adapters
  URI org.parchmint.adapters
  VERSION 1.0
  DEPENDENCIES QtQml.Models
  OUTPUT_DIRECTORY ${CMAKE_CURRENT_BINARY_DIR}/org/parchmint/adapters
  SOURCES
    editor_adapter.cpp editor_adapter.h
    markdown_highlighter.cpp markdown_highlighter.h
    semantic_object_renderer.cpp semantic_object_renderer.h
    models/outline_model.cpp models/outline_model.h
    models/cards_model.cpp models/cards_model.h)
target_include_directories(parchmint_adapters PUBLIC
  ${CMAKE_CURRENT_SOURCE_DIR}
  ${CMAKE_CURRENT_SOURCE_DIR}/models)
target_link_libraries(parchmint_adapters PUBLIC Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick)

qt_add_executable(parchmint main.cpp)
set(parchmint_qml_sources
  "${CMAKE_SOURCE_DIR}/app/qml/Main.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/DesignTokens.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/IconButton.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/FormattingBar.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/BinderPane.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/OutlineView.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/CardsView.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/InspectorPane.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/StylePicker.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/StyleManager.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/SourceEditor.qml"
  "${CMAKE_SOURCE_DIR}/app/qml/components/PaneHost.qml")
set_source_files_properties(${parchmint_qml_sources} PROPERTIES HEADER_FILE_ONLY TRUE)
target_sources(parchmint PRIVATE ${parchmint_qml_sources})
foreach(qml_file IN LISTS parchmint_qml_sources)
  file(RELATIVE_PATH qml_alias "${CMAKE_SOURCE_DIR}/app/qml" "${qml_file}")
  set_source_files_properties("${qml_file}" PROPERTIES QT_RESOURCE_ALIAS "${qml_alias}")
endforeach()
# CXX-Qt embeds the production module. This second resource is scanner input so
# Qt's deployment API can discover and package all transitive Qt QML imports.
qt_add_resources(parchmint "parchmint_qml_deploy_scan"
  PREFIX "/parchmint-qml-deploy-scan"
  FILES ${parchmint_qml_sources})
set_target_properties(parchmint PROPERTIES
  MACOSX_BUNDLE TRUE
  MACOSX_BUNDLE_BUNDLE_NAME "ParchMint"
  MACOSX_BUNDLE_GUI_IDENTIFIER "org.parchmint.ParchMint"
  MACOSX_BUNDLE_INFO_PLIST "${CMAKE_SOURCE_DIR}/packaging/macos/Info.plist.in"
  WIN32_EXECUTABLE TRUE)
if(APPLE)
  set_property(TARGET parchmint PROPERTY INSTALL_RPATH "@executable_path/../Frameworks")
elseif(UNIX)
  set_property(TARGET parchmint PROPERTY INSTALL_RPATH "$ORIGIN/../lib")
endif()
target_compile_definitions(parchmint PRIVATE PARCHMINT_VERSION="${PROJECT_VERSION}")
set(parchmint_icon "${CMAKE_SOURCE_DIR}/packaging/icons/org.parchmint.ParchMint.svg")
set_source_files_properties(${parchmint_icon} PROPERTIES QT_RESOURCE_ALIAS "org.parchmint.ParchMint.svg")
qt_add_resources(parchmint "parchmint_icons"
  PREFIX "/"
  FILES ${parchmint_icon})
qt_add_resources(parchmint "parchmint_ui_icons"
  PREFIX "/icons"
  BASE "${CMAKE_SOURCE_DIR}/app/qml/icons"
  FILES
    "${CMAKE_SOURCE_DIR}/app/qml/icons/binder.svg"
    "${CMAKE_SOURCE_DIR}/app/qml/icons/inspector.svg"
    "${CMAKE_SOURCE_DIR}/app/qml/icons/pin.svg"
    "${CMAKE_SOURCE_DIR}/app/qml/icons/search.svg"
    "${CMAKE_SOURCE_DIR}/app/qml/icons/close.svg"
    "${CMAKE_SOURCE_DIR}/app/qml/icons/chevron.svg"
    "${CMAKE_SOURCE_DIR}/app/qml/icons/document.svg")

target_link_libraries(parchmint PRIVATE
  parchmint_qml
  parchmint_adaptersplugin
  Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2)

qt_import_plugins(parchmint
  INCLUDE_BY_TYPE platforms
    Qt6::QXcbIntegrationPlugin
    Qt6::QOffscreenIntegrationPlugin
    Qt6::QWaylandIntegrationPlugin
    Qt6::QWaylandEglPlatformIntegrationPlugin
  EXCLUDE_BY_TYPE networkinformation tls qmltooling)
qt_import_qml_plugins(parchmint)

qt_generate_deploy_qml_app_script(
  TARGET parchmint
  OUTPUT_SCRIPT parchmint_deploy_script
  NO_TRANSLATIONS
  NO_UNSUPPORTED_PLATFORM_ERROR)
install(SCRIPT ${parchmint_deploy_script})

qt_add_executable(parchmint-editor-tests
  ${CMAKE_SOURCE_DIR}/tests/qt/tst_editor_adapter.cpp)
target_link_libraries(parchmint-editor-tests PRIVATE
  parchmint_adapters Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2 Qt6::Test)
target_include_directories(parchmint-editor-tests PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
add_test(NAME editor-adapter COMMAND parchmint-editor-tests)
set_tests_properties(editor-adapter PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 20)

qt_add_executable(parchmint-outline-model-tests
  ${CMAKE_SOURCE_DIR}/tests/qt/tst_outline_model.cpp)
target_link_libraries(parchmint-outline-model-tests PRIVATE
  parchmint_adapters Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::Test)
target_include_directories(parchmint-outline-model-tests PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
add_test(NAME outline-model COMMAND parchmint-outline-model-tests)
set_tests_properties(outline-model PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 20)

qt_add_executable(parchmint-editor-benchmark
  ${CMAKE_SOURCE_DIR}/tests/qt/tst_editor_benchmark.cpp)
target_link_libraries(parchmint-editor-benchmark PRIVATE Qt6::Core Qt6::Gui Qt6::Test)
target_link_libraries(parchmint-editor-benchmark PRIVATE parchmint_adapters Qt6::Qml Qt6::Quick)
target_include_directories(parchmint-editor-benchmark PRIVATE ${CMAKE_CURRENT_SOURCE_DIR})
add_custom_target(editor-benchmark
  COMMAND ${CMAKE_COMMAND} -E env QT_QPA_PLATFORM=offscreen $<TARGET_FILE:parchmint-editor-benchmark>
  DEPENDS parchmint-editor-benchmark
  USES_TERMINAL)

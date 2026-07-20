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
    models/outline_model.cpp models/outline_model.h)
target_include_directories(parchmint_adapters PUBLIC ${CMAKE_CURRENT_SOURCE_DIR})
target_link_libraries(parchmint_adapters PUBLIC Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick)

qt_add_executable(parchmint main.cpp)

target_link_libraries(parchmint PRIVATE
  parchmint_qml
  parchmint_adaptersplugin
  Qt6::Core Qt6::Gui Qt6::Qml Qt6::Quick Qt6::QuickControls2)

qt_import_qml_plugins(parchmint)

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

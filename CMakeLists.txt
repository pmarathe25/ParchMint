cmake_minimum_required(VERSION 3.24)
project(ParchMint VERSION 0.1.0 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)
set(CMAKE_AUTOMOC ON)
set(CMAKE_AUTORCC ON)
set(CMAKE_EXPORT_COMPILE_COMMANDS ON)

if(MSVC)
  set(CMAKE_MSVC_RUNTIME_LIBRARY "MultiThreadedDLL")
endif()

# Stage 01 pins one tested patch release. Later security patch updates require CI
# evidence but do not change the Qt 6.8 architecture decision.
find_package(Qt6 6.8.3 EXACT REQUIRED COMPONENTS Core Gui Network Qml Quick QuickControls2 Sql Test QuickTest)
qt_policy(SET QTP0001 NEW)

if(MSVC)
  set_property(TARGET
    Qt6::Core Qt6::Gui Qt6::Network Qt6::Qml Qt6::Quick
    Qt6::QuickControls2 Qt6::Sql Qt6::Test Qt6::QuickTest
    PROPERTY MAP_IMPORTED_CONFIG_DEBUG "RELEASE")
endif()

if(PARCHMINT_CONFIGURE_ONLY)
  add_custom_target(qmllint COMMAND ${CMAKE_COMMAND} -E echo "configure-only: qmllint skipped")
  return()
endif()

include(FetchContent)
find_package(CxxQt 0.9 QUIET)
if(NOT CxxQt_FOUND)
  FetchContent_Declare(
    CxxQt
    GIT_REPOSITORY https://github.com/KDAB/cxx-qt-cmake.git
    GIT_TAG 0.9.1
    GIT_SHALLOW TRUE)
  FetchContent_MakeAvailable(CxxQt)
endif()

cxx_qt_import_crate(
  MANIFEST_PATH crates/parchmint-bridge/Cargo.toml
  CRATES parchmint_bridge
  LOCKED
  QT_MODULES Qt6::Core Qt6::Gui Qt6::Network Qt6::Qml Qt6::Quick Qt6::QuickControls2)

cxx_qt_import_qml_module(parchmint_qml
  URI "org.parchmint.app"
  SOURCE_CRATE parchmint_bridge)

enable_testing()
add_subdirectory(app/cpp)

add_test(NAME parchmint-smoke COMMAND parchmint --smoke-test)
set_tests_properties(parchmint-smoke PROPERTIES ENVIRONMENT "QT_QPA_PLATFORM=offscreen" TIMEOUT 15)

add_custom_target(
  qmllint
  COMMAND Qt6::qmllint -I ${CMAKE_CURRENT_BINARY_DIR}/cxxqt/qml_modules
          -I ${CMAKE_CURRENT_BINARY_DIR}/app/cpp
          ${CMAKE_CURRENT_SOURCE_DIR}/app/qml/Main.qml
          ${CMAKE_CURRENT_SOURCE_DIR}/app/qml/components/FormattingBar.qml
          ${CMAKE_CURRENT_SOURCE_DIR}/app/qml/components/SourceEditor.qml
          ${CMAKE_CURRENT_SOURCE_DIR}/app/qml/components/StyleManager.qml
          ${CMAKE_CURRENT_SOURCE_DIR}/app/qml/components/StylePicker.qml
  DEPENDS parchmint_adaptersplugin
  VERBATIM)

add_custom_target(run COMMAND parchmint DEPENDS parchmint USES_TERMINAL)

install(TARGETS parchmint RUNTIME DESTINATION bin BUNDLE DESTINATION .)

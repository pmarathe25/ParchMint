//! Builds the generated CXX-Qt QML module and bridge adapter.

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    let module = QmlModule::new("org.parchmint.app")
        .version(1, 0)
        .qml_file("qml/Main.qml")
        .qml_file(QmlFile::from("qml/components/DesignTokens.qml").singleton(true))
        .qml_file("qml/components/FormattingBar.qml")
        .qml_file("qml/components/BinderPane.qml")
        .qml_file("qml/components/OutlineView.qml")
        .qml_file("qml/components/CardsView.qml")
        .qml_file("qml/components/InspectorPane.qml")
        .qml_file("qml/components/StylePicker.qml")
        .qml_file("qml/components/StyleManager.qml")
        .qml_file("qml/components/SourceEditor.qml");

    CxxQtBuilder::new_qml_module(module)
        .qt_module("Network")
        .file("src/backend.rs")
        .build();
}

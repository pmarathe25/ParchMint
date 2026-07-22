//! Builds the generated CXX-Qt QML module and bridge adapter.

use cxx_qt_build::{CxxQtBuilder, QmlFile, QmlModule};

fn main() {
    let module = QmlModule::new("org.parchmint.app")
        .version(1, 0)
        .qml_file("qml/Main.qml")
        .qml_file(QmlFile::from("qml/components/DesignTokens.qml").singleton(true))
        .qml_file("qml/components/FormattingBar.qml")
        .qml_file("qml/components/BinderPane.qml")
        .qml_file("qml/components/EditorWorkspace.qml")
        .qml_file("qml/components/CardsView.qml")
        .qml_file("qml/components/StylePicker.qml")
        .qml_file("qml/components/StyleManager.qml")
        .qml_file("qml/components/SourceEditor.qml")
        .qml_file("qml/components/PaneHost.qml");

    // `file` accepts only Rust CXX-Qt bridge sources. Keep the small native PDF
    // renderer in the same C++ compilation unit as the generated bridge code.
    unsafe {
        CxxQtBuilder::new_qml_module(module)
            .file("src/backend.rs")
            .cc_builder(|cc| {
                cc.file("src/pdf_renderer.cpp");
                cc.file("src/path_helper.cpp");
            })
            .build();
    }
}

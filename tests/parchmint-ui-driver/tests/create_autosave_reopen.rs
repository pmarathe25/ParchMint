use std::fs;

use parchmint_desktop::{
    DesktopInteractionHarness, HarnessWindow, LaunchRequest, ProductionObservation,
};
use parchmint_ui_driver::IsolatedRun;

#[test]
fn user_can_create_edit_autosave_close_and_reopen_a_project() {
    let run = IsolatedRun::new("create-autosave-reopen").expect("isolated run");
    let project = run.root().join("flow-novel.parchmint");
    let marker = "The harness wrote this sentence.";

    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("launch application");
    harness
        .click_text(HarnessWindow::Launcher, "Create Project")
        .expect("open create form");
    harness
        .type_into(HarnessWindow::Launcher, "Project title", "Flow Novel")
        .expect("enter title");
    harness
        .type_into(
            HarnessWindow::Launcher,
            "Project destination",
            project.to_string_lossy(),
        )
        .expect("enter destination");
    harness
        .type_into(HarnessWindow::Launcher, "Author (optional)", "UI Harness")
        .expect("enter author");
    harness
        .click_text(HarnessWindow::Launcher, "Create and Open")
        .expect("create project");
    assert!(
        harness
            .has_window(HarnessWindow::Project)
            .expect("query project window")
    );

    harness
        .type_at(HarnessWindow::Project, (500.0, 300.0), marker)
        .expect("type in mounted editor");
    assert!(
        harness
            .active_editor_body()
            .expect("read mounted editor projection")
            .contains(marker)
    );
    harness
        .elapse_autosave_idle()
        .expect("advance and complete autosave");
    let persisted = canonical_document_bodies(&project);
    assert!(persisted.iter().any(|body| body.contains(marker)));
    harness
        .close(HarnessWindow::Project)
        .expect("close project");
    assert!(
        harness
            .observations()
            .iter()
            .any(|observation| matches!(observation, ProductionObservation::WindowClosed(_)))
    );
    harness.shutdown().expect("stop first application instance");

    let reopened = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("relaunch application");
    reopened
        .click_text(HarnessWindow::Launcher, "Flow Novel")
        .expect("open recent project");
    assert!(
        reopened
            .active_editor_body()
            .expect("read reopened editor projection")
            .contains(marker)
    );
    reopened
        .close(HarnessWindow::Project)
        .expect("close reopened project");
    reopened.shutdown().expect("stop reopened application");
}

fn canonical_document_bodies(project: &std::path::Path) -> Vec<String> {
    ["manuscript", "research"]
        .into_iter()
        .flat_map(|directory| {
            fs::read_dir(project.join(directory))
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
        })
        .filter(|entry| {
            entry
                .path()
                .extension()
                .is_some_and(|value| value == "html")
        })
        .map(|entry| fs::read_to_string(entry.path()).expect("read autosaved canonical document"))
        .collect()
}

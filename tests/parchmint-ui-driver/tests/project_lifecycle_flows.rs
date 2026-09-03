use std::path::Path;

use parchmint_desktop::{
    DesktopInteractionHarness, HarnessTarget, HarnessWindow, LaunchRequest, RibbonDestination,
};
use parchmint_ui_driver::IsolatedRun;

/// Export preferences are project context, so a writer can configure title
/// emission once and retain it across sessions.
#[test]
fn export_settings_survive_a_project_restart() {
    let run = IsolatedRun::new("lifecycle-export-persistence").expect("isolated run");
    let project = run.root().join("export-preferences.parchmint");
    let harness = create_project(&run, &project, "Export Preferences");
    create_document(&harness, "Manuscript", "Opening");

    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Export),
        )
        .expect("open export settings");
    harness
        .click_text(HarnessWindow::Project, "Browse…")
        .expect("choose export destination");
    harness
        .click_text(HarnessWindow::Project, "Project default")
        .expect("change title emission preference");
    assert!(contains(&harness, "Include"));
    harness
        .close(HarnessWindow::Project)
        .expect("close configured project");
    harness.shutdown().expect("stop configured application");

    let reopened = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("relaunch configured application");
    reopened
        .click_text(HarnessWindow::Launcher, "Export Preferences")
        .expect("reopen configured project");
    reopened
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Export),
        )
        .expect("reopen export settings");
    assert!(contains(&reopened, "Include"));
    reopened
        .close(HarnessWindow::Project)
        .expect("close export project");
    reopened.shutdown().expect("stop export application");
}

fn create_project(run: &IsolatedRun, project: &Path, title: &str) -> DesktopInteractionHarness {
    let harness = DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher())
        .expect("launch application");
    harness
        .click_text(HarnessWindow::Launcher, "Create Project")
        .expect("open create form");
    harness
        .type_into(HarnessWindow::Launcher, "Project title", title)
        .expect("enter project title");
    harness
        .type_into(
            HarnessWindow::Launcher,
            "Project destination",
            project.to_string_lossy(),
        )
        .expect("enter project destination");
    harness
        .click_text(HarnessWindow::Launcher, "Create and Open")
        .expect("create project");
    harness
}

fn create_document(harness: &DesktopInteractionHarness, parent: &str, title: &str) {
    harness
        .right_click_text(HarnessWindow::Project, parent)
        .expect("open group menu");
    harness
        .click_text(HarnessWindow::Project, "Create document")
        .expect("create document");
    harness
        .replace_text_and_submit(HarnessWindow::Project, "Untitled", title)
        .expect("name document");
}

fn contains(harness: &DesktopInteractionHarness, text: &str) -> bool {
    harness
        .contains_text(HarnessWindow::Project, text)
        .expect("query project surface")
}

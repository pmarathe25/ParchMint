use parchmint_desktop::{
    DesktopInteractionHarness, HarnessTarget, HarnessWindow, LaunchRequest, ProductionFaultKind,
    ProductionFaultPoint, RibbonDestination,
};
use parchmint_ui_driver::{IsolatedRun, create_document, create_project};

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

fn contains(harness: &DesktopInteractionHarness, text: &str) -> bool {
    harness
        .contains_text(HarnessWindow::Project, text)
        .expect("query project surface")
}

#[test]
fn launcher_open_errors_fail_the_action_and_allow_retry() {
    let run = IsolatedRun::new("launcher-error-retry").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Retry Novel");
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();

    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::launcher()).unwrap();
    reopened.fail_next(ProductionFaultPoint::ProjectOpen, ProductionFaultKind::Io);
    let error = reopened
        .click_text(HarnessWindow::Launcher, "Retry Novel")
        .expect_err("an error displayed by the launcher must fail the action");
    assert!(error.to_string().contains("application reported an error"));
    reopened
        .click_text(HarnessWindow::Launcher, "Retry Novel")
        .unwrap();
    assert!(!reopened.hierarchy_titles().unwrap().is_empty());
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
}

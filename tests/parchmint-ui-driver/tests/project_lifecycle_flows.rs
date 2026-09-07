use parchmint_desktop::{
    DesktopInteractionHarness, HarnessTarget, HarnessWindow, LaunchRequest, ProductionFaultKind,
    ProductionFaultPoint, RibbonDestination,
};
use parchmint_ui_driver::{IsolatedRun, create_document, create_project};

#[test]
fn style_fields_keep_multicharacter_values_until_submitted() {
    let run = IsolatedRun::new("style-field-editing").unwrap();
    let project = run.root().join("styles.parchmint");
    let harness = create_project(&run, &project, "Style editing");
    harness
        .resize(HarnessWindow::Project, 1440.0, 900.0)
        .unwrap();
    harness
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Settings),
        )
        .unwrap();
    harness
        .click_text(HarnessWindow::Project, "Styles")
        .unwrap();
    harness.click_text(HarnessWindow::Project, "Body").unwrap();
    let before = std::fs::read(project.join("styles.css")).ok();
    for (field, value) in [
        ("Enter font family", "Source Serif 4"),
        ("Enter font size (pt)", "12.5"),
    ] {
        harness
            .type_into(HarnessWindow::Project, field, value)
            .unwrap();
        if field == "Enter font family" {
            assert_eq!(std::fs::read(project.join("styles.css")).ok(), before);
        }
        harness
            .press_key(HarnessWindow::Project, parchmint_desktop::HarnessKey::Enter)
            .unwrap();
    }
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();
    let css = std::fs::read_to_string(project.join("styles.css")).unwrap();
    assert!(css.contains("Source Serif 4"), "{css}");
    assert!(css.contains("12.5pt"), "{css}");
}

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
        .click_target(HarnessWindow::Project, HarnessTarget::ExportTitles)
        .expect("open chapter title options");
    harness
        .click_target_offset(
            HarnessWindow::Project,
            HarnessTarget::ExportTitles,
            (0.5, 3.5),
        )
        .expect("exclude chapter titles");
    harness
        .close(HarnessWindow::Project)
        .expect("close configured project");
    harness.shutdown().expect("stop configured application");
    assert!(
        std::fs::read_to_string(project.join("project.toml"))
            .unwrap()
            .contains(r#"export-emit-titles = "disabled""#)
    );

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
    assert!(
        std::fs::read_to_string(project.join("project.toml"))
            .unwrap()
            .contains(r#"export-emit-titles = "disabled""#)
    );
    reopened
        .close(HarnessWindow::Project)
        .expect("close export project");
    reopened.shutdown().expect("stop export application");
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

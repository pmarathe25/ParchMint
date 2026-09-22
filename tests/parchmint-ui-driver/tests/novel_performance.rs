//! Opt-in release measurements; no machine-dependent assertions in normal CI.
use std::{collections::BTreeMap, fs, path::Path, time::Instant};

use parchmint_desktop::{
    DesktopInteractionHarness, EditorPane, HarnessKey, HarnessTarget, HarnessWindow, LaunchRequest,
};
use parchmint_domain::{
    DocumentId, NodeId, Project, ProjectCommand, ProjectId, apply_project_command,
};
use parchmint_project_format::{CanonicalProjectPathMap, ProjectFormatCodec};
use parchmint_ui_driver::IsolatedRun;
use serde_json::json;

const WINDOW: HarnessWindow = HarnessWindow::Project;

fn require_release_build() {
    #[cfg(debug_assertions)]
    panic!("measure with --release");
}

fn workload() -> (usize, usize, bool) {
    match std::env::var("PARCHMINT_BENCH_LAYOUT")
        .as_deref()
        .unwrap_or("chapters")
    {
        "chapters" => (10, 5_000, false),
        "single" => (1, 50_000, false),
        "small" => (10, 80, false),
        "cards" => (200, 80, false),
        "formatted" => (1, 50_000, true),
        other => panic!("unknown benchmark layout: {other}"),
    }
}

fn seed(path: &Path, chapters: usize, words: usize, formatted: bool) {
    fs::create_dir(path).unwrap();
    fs::create_dir(path.join(".parchmint")).unwrap();
    fs::write(path.join(".parchmint/root-id"), "0000000000000001\n").unwrap();
    let sentence = "The harbor lantern shines through the rain tonight.";
    let tokens: Vec<_> = sentence
        .split_whitespace()
        .cycle()
        .take(words - 1)
        .collect();
    let body = format!(
        "<p>Benchmarkanchor.</p>{}",
        tokens
            .chunks(80)
            .map(|p| {
                if formatted {
                    format!(
                        "<p><strong>{}</strong> <em>{}</em> {}</p>",
                        p[..2].join(" "),
                        p[2..4].join(" "),
                        p[4..].join(" ")
                    )
                } else {
                    format!("<p>{}</p>", p.join(" "))
                }
            })
            .collect::<String>()
    );
    assert_eq!(tokens.len() + 1, words);
    let mut project = Project::new(ProjectId::from_bytes([0x91; 16]));
    project.display_title = format!("{}-word novel", chapters * words);
    let mut bodies = BTreeMap::new();
    for index in 0..chapters {
        let id = DocumentId::from_bytes([index as u8 + 1; 16]);
        project = apply_project_command(
            &project,
            project.revision,
            ProjectCommand::create_document(
                NodeId::from_bytes([index as u8 + 30; 16]),
                id,
                NodeId::manuscript_root(),
                index,
                format!("Chapter {:02}", index + 1),
            ),
        )
        .unwrap()
        .project;
        bodies.insert(id, body.clone());
    }
    let encoded = ProjectFormatCodec::default()
        .encode_domain_project(
            &project,
            &bodies,
            &BTreeMap::new(),
            &CanonicalProjectPathMap::default(),
        )
        .unwrap();
    for resource in encoded.resources.into_values() {
        let destination = path.join(resource.path.as_str());
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::write(destination, resource.bytes).unwrap();
    }
}

#[test]
#[ignore = "generate a disposable novel fixture at PARCHMINT_BENCH_FIXTURE"]
fn seed_novel_benchmark() {
    let (chapters, words, formatted) = workload();
    seed(
        Path::new(&std::env::var_os("PARCHMINT_BENCH_FIXTURE").expect("new fixture directory")),
        chapters,
        words,
        formatted,
    );
}

fn ms(start: Instant) -> f64 {
    start.elapsed().as_secs_f64() * 1000.0
}

fn samples(count: usize, mut action: impl FnMut()) -> serde_json::Value {
    let mut times: Vec<_> = (0..count)
        .map(|_| {
            let start = Instant::now();
            action();
            ms(start)
        })
        .collect();
    times.sort_by(f64::total_cmp);
    json!({"n": count, "p50_ms": times[count / 2], "p95_ms": times[count * 95 / 100], "max_ms": times[count - 1]})
}

fn memory() -> BTreeMap<String, u64> {
    fs::read_to_string("/proc/self/status")
        .unwrap_or_default()
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            ["VmRSS", "VmHWM"].contains(&key).then(|| {
                (
                    key.to_owned(),
                    value.split_whitespace().next().unwrap().parse().unwrap(),
                )
            })
        })
        .collect()
}

#[test]
#[ignore = "opt-in full-application release timing; excludes native presentation"]
fn novel_application_performance() {
    require_release_build();
    let (chapters, words, formatted) = workload();
    let run = IsolatedRun::new("novel-performance").unwrap();
    let project = run.root().join("novel.parchmint");
    seed(&project, chapters, words, formatted);
    let start = Instant::now();
    let harness =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    let open_ms = ms(start);
    let loaded_memory = memory();
    harness
        .click_target(WINDOW, HarnessTarget::EditorPrimary)
        .unwrap();
    harness
        .select_editor_text(WINDOW, EditorPane::Primary, "Benchmarkanchor.")
        .unwrap();
    harness.press_key(WINDOW, HarnessKey::ArrowLeft).unwrap();
    let typing_one = samples(256, || harness.type_focused(WINDOW, "x").unwrap());
    let title = harness.active_editor_tab_title().unwrap();
    harness.right_click_text(WINDOW, title).unwrap();
    let start = Instant::now();
    harness.click_text(WINDOW, "Open beside").unwrap();
    let split_ms = ms(start);
    assert!(harness.editor_panes_share_session().unwrap());
    harness
        .click_target(WINDOW, HarnessTarget::EditorPrimary)
        .unwrap();
    harness
        .select_editor_text(WINDOW, EditorPane::Primary, "Benchmarkanchor.")
        .unwrap();
    harness.press_key(WINDOW, HarnessKey::ArrowLeft).unwrap();
    let typing_split = samples(256, || harness.type_focused(WINDOW, "x").unwrap());
    let typed_memory = memory();
    let marker = "x".repeat(512);
    assert!(harness.active_editor_body().unwrap().contains(&marker));
    let selection = samples(100, || {
        harness
            .press_shift_key(WINDOW, HarnessKey::ArrowRight)
            .unwrap()
    });
    let scroll = samples(100, || {
        harness
            .scroll_target_by(WINDOW, HarnessTarget::EditorPrimary, -48.0)
            .unwrap()
    });
    harness
        .select_editor_text(WINDOW, EditorPane::Primary, "Benchmarkanchor.")
        .unwrap();
    harness.press_key(WINDOW, HarnessKey::ArrowLeft).unwrap();
    let mut save_times = Vec::new();
    for index in 0..10 {
        harness
            .type_focused(WINDOW, format!("savedmarker{index} "))
            .unwrap();
        let start = Instant::now();
        harness.press_command_key(WINDOW, 's').unwrap();
        save_times.push(ms(start));
    }
    let mut index = 0;
    let switching = samples(20, || {
        harness
            .click_text(WINDOW, format!("Chapter {:02}", index % chapters + 1))
            .unwrap();
        index += 1;
    });
    let visited_memory = memory();
    harness.close(WINDOW).unwrap();
    harness.shutdown().unwrap();
    let start = Instant::now();
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    let reopen_ms = ms(start);
    reopened.click_text(WINDOW, "Chapter 01").unwrap();
    let body = reopened.active_editor_body().unwrap();
    assert!(body.contains(&marker));
    for index in 0..10 {
        assert!(body.contains(&format!("savedmarker{index}")));
    }
    if formatted {
        assert_eq!(body.matches("<strong>").count(), (words - 1).div_ceil(80));
        assert_eq!(body.matches("<em>").count(), (words - 1).div_ceil(80));
    }
    reopened.close(WINDOW).unwrap();
    reopened.shutdown().unwrap();
    eprintln!(
        "NOVEL_BENCH {}",
        json!({
            "chapters": chapters, "words": chapters * words, "formatted": formatted,
            "open_ms": open_ms,
            "typing_one": typing_one, "typing_split": typing_split, "split_ms": split_ms,
            "selection": selection, "scroll": scroll, "save_ms": save_times,
            "switching": switching, "reopen_ms": reopen_ms,
            "loaded_memory_kib": loaded_memory, "typed_memory_kib": typed_memory,
            "visited_memory_kib": visited_memory,
        })
    );
}

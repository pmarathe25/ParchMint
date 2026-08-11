use std::collections::BTreeMap;
use std::sync::Mutex;

use parchmint_export_api::{
    DocumentId, ExportDefaults, ExportError, ExportHandle, ExportNode, ExportPlan, ExportRequest,
    ExportRunOptions, ExportSettings, ExportSink, ExportSource, ExportStyleCatalog,
    ExportTargetCapability, InheritedSetting, ProjectSnapshot,
};

use super::HtmlExporter;

fn document_id(value: u8) -> DocumentId {
    DocumentId::from_bytes([value; 16])
}

fn document(id: u8, title: &str, body: &str) -> (ExportNode, (DocumentId, ExportSource)) {
    let id = document_id(id);
    (
        ExportNode::document(id, title, ExportSettings::default()),
        (
            id,
            ExportSource {
                revision: 1.into(),
                body: body.into(),
            },
        ),
    )
}

fn plan(
    manuscript: Vec<ExportNode>,
    sources: BTreeMap<DocumentId, ExportSource>,
    css: &str,
) -> ExportPlan {
    ExportPlan::build(
        ExportRequest::new("manuscript.html", ExportRunOptions::default()),
        &ProjectSnapshot::new(
            ExportStyleCatalog::new(css),
            ExportDefaults::default(),
            manuscript,
            sources,
        ),
    )
    .expect("fixture creates a valid plan")
}

#[derive(Default)]
struct CaptureSink {
    chunks: Vec<Vec<u8>>,
    complete: bool,
    aborted: bool,
    cancel_after: Option<usize>,
    handle: Option<ExportHandle>,
}

impl ExportSink for CaptureSink {
    fn start(&mut self, _: &ExportTargetCapability) -> Result<(), ExportError> {
        Ok(())
    }

    fn write_chunk(&mut self, bytes: &[u8]) -> Result<(), ExportError> {
        self.chunks.push(bytes.to_vec());
        if self.cancel_after == Some(self.chunks.len())
            && let Some(handle) = &self.handle
        {
            let _ = handle.cancel();
        }
        Ok(())
    }

    fn finish(&mut self) -> Result<(), ExportError> {
        self.complete = true;
        Ok(())
    }

    fn abort(&mut self) {
        self.aborted = true;
    }
}

fn bytes(sink: &CaptureSink) -> Vec<u8> {
    sink.chunks.iter().flatten().copied().collect()
}

fn render(plan: &ExportPlan) -> String {
    let mut sink = CaptureSink::default();
    HtmlExporter
        .render(
            plan,
            &mut sink,
            &ExportHandle::new(),
            &parchmint_export_api::IgnoreExportProgress,
        )
        .expect("render succeeds");
    String::from_utf8(bytes(&sink)).expect("UTF-8 HTML")
}

#[test]
fn export_is_deterministic_golden_html_bytes_and_embeds_project_css() {
    let (first, first_source) = document(1, "Chapter & One", "Hello");
    let (second, second_source) = document(2, "Second", "World");
    let expected = br#"<!doctype html><html><head><meta charset="utf-8"><style>p { color: #123456; }</style></head><body><h1>Chapter &amp; One</h1><article><h2>Chapter &amp; One</h2><p>Hello</p></article><article><h2>Second</h2><p>World</p></article></body></html>"#;

    let fixture = plan(
        vec![ExportNode::group(
            "Chapter & One",
            ExportSettings::default(),
            vec![first, second],
        )],
        BTreeMap::from([first_source, second_source]),
        "p { color: #123456; }",
    );

    assert_eq!(render(&fixture).as_bytes(), expected);
    assert_eq!(render(&fixture).as_bytes(), expected);
}

#[test]
fn html_css_and_links_are_sanitized_without_remote_content() {
    let body = r#"<p title="a & b">5 < 6 & 7</p><a href="https://example.test/?q=1&x=2">safe</a><a href="javascript:alert(1)">bad</a><a href="file:///etc/passwd">local</a><script src="https://cdn.example.test/x.js"></script><img src="https://cdn.example.test/x.png" onload="steal()">"#;
    let (node, source) = document(1, "A < B", body);
    let html = render(&plan(
        vec![node],
        BTreeMap::from([source]),
        "body { background: url(https://cdn.example.test/x.png); }",
    ));

    assert!(html.contains("A &lt; B"));
    assert!(html.contains("title=\"a &amp; b\""));
    assert!(html.contains("5 &lt; 6 &amp; 7"));
    assert!(html.contains("href=\"https://example.test/?q=1&amp;x=2\""));
    assert!(!html.contains("javascript:"));
    assert!(!html.contains("file:///"));
    assert!(!html.contains("<script"));
    assert!(!html.contains("onload="));
    assert!(!html.contains("cdn.example.test"));
}

#[test]
fn page_breaks_are_structural_and_existing_document_titles_are_not_duplicated() {
    let (titled, titled_source) = document(1, "Title", "<h1>Title</h1><p>body</p>");
    let (_, next_source) = document(2, "Next", "body");
    let next = ExportNode::document(
        document_id(2),
        "Next",
        ExportSettings {
            start_new_page: InheritedSetting::Enabled,
            ..ExportSettings::default()
        },
    );
    let html = render(&plan(
        vec![titled, next],
        BTreeMap::from([titled_source, next_source]),
        "",
    ));

    assert_eq!(html.matches(">Title<").count(), 1);
    assert!(html.contains("<div class=\"page-break\" aria-hidden=\"true\"></div>"));
    assert!(!html.contains(">PageBreak<"));
}

#[test]
fn writes_are_chunked_and_cancellation_aborts_before_completion() {
    let body = "a".repeat(16 * 1024);
    let handle = ExportHandle::new();
    let mut sink = CaptureSink {
        cancel_after: Some(2),
        handle: Some(handle.clone()),
        ..CaptureSink::default()
    };
    let result = HtmlExporter.render(
        &plan(
            vec![ExportNode::document(
                document_id(1),
                "Title",
                ExportSettings::default(),
            )],
            BTreeMap::from([(
                document_id(1),
                ExportSource {
                    revision: 1.into(),
                    body,
                },
            )]),
            "",
        ),
        &mut sink,
        &handle,
        &parchmint_export_api::IgnoreExportProgress,
    );

    assert_eq!(result, Err(ExportError::Cancelled));
    assert!(
        sink.chunks.len() > 1,
        "large output must use bounded chunks"
    );
    assert!(sink.aborted);
    assert!(!sink.complete);
}

#[derive(Default)]
struct RecordingProgress(Mutex<Vec<parchmint_export_api::ExportProgress>>);

impl parchmint_export_api::ExportProgressSink for RecordingProgress {
    fn report(&self, progress: parchmint_export_api::ExportProgress) {
        self.0.lock().expect("progress log").push(progress);
    }
}

#[test]
fn progress_is_determinate_for_each_planned_semantic_item() {
    let progress = RecordingProgress::default();
    let mut sink = CaptureSink::default();
    HtmlExporter
        .render(
            &plan(
                vec![
                    ExportNode::document(document_id(1), "First", ExportSettings::default()),
                    ExportNode::document(document_id(2), "Second", ExportSettings::default()),
                ],
                BTreeMap::from([
                    (
                        document_id(1),
                        ExportSource {
                            revision: 1.into(),
                            body: "one".into(),
                        },
                    ),
                    (
                        document_id(2),
                        ExportSource {
                            revision: 1.into(),
                            body: "two".into(),
                        },
                    ),
                ]),
                "",
            ),
            &mut sink,
            &ExportHandle::new(),
            &progress,
        )
        .expect("render");

    assert_eq!(
        *progress.0.lock().expect("progress log"),
        vec![
            parchmint_export_api::ExportProgress::Rendering {
                completed: 0,
                total: 2,
            },
            parchmint_export_api::ExportProgress::Rendering {
                completed: 1,
                total: 2,
            },
            parchmint_export_api::ExportProgress::Rendering {
                completed: 2,
                total: 2,
            },
            parchmint_export_api::ExportProgress::Committing,
        ]
    );
}

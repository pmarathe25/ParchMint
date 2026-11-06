use std::fs;

use parchmint_desktop::{
    DesktopInteractionHarness, HarnessTarget, HarnessWindow, LaunchRequest, RibbonDestination,
};
use parchmint_ui_driver::{IsolatedRun, create_document, create_project};

fn local_assistant() -> (
    String,
    std::sync::mpsc::Receiver<String>,
    std::thread::JoinHandle<()>,
) {
    local_assistant_with_check(false)
}

fn local_assistant_with_check(
    check: bool,
) -> (
    String,
    std::sync::mpsc::Receiver<String>,
    std::thread::JoinHandle<()>,
) {
    local_assistant_with_check_requests(check, 1)
}

fn local_assistant_with_check_requests(
    check: bool,
    count: usize,
) -> (
    String,
    std::sync::mpsc::Receiver<String>,
    std::thread::JoinHandle<()>,
) {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let (sender, receiver) = mpsc::channel();
    let server = std::thread::spawn(move || {
        for _ in 0..count {
            let deadline = Instant::now() + Duration::from_secs(60);
            let (mut stream, _) = loop {
                match listener.accept() {
                    Ok(connection) => break connection,
                    Err(error)
                        if error.kind() == std::io::ErrorKind::WouldBlock
                            && Instant::now() < deadline =>
                    {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                    Err(error) => panic!("local provider: {error}"),
                }
            };
            stream.set_nonblocking(false).unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut bytes = Vec::new();
            let mut buffer = [0; 4096];
            let header_end = loop {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
                if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                    break index + 4;
                }
            };
            let header = std::str::from_utf8(&bytes[..header_end]).unwrap();
            let length = header
                .lines()
                .find_map(|line| {
                    line.split_once(':')
                        .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                        .and_then(|(_, value)| value.trim().parse::<usize>().ok())
                })
                .unwrap();
            while bytes.len() < header_end + length {
                let count = stream.read(&mut buffer).unwrap();
                assert!(count > 0);
                bytes.extend_from_slice(&buffer[..count]);
            }
            let request =
                String::from_utf8(bytes[header_end..header_end + length].to_vec()).unwrap();
            let answer = if check && request.contains("Suggest a replacement") {
                serde_json::json!({"replacement":"Ivo once carried the silver key."}).to_string()
            } else if check {
                let body: serde_json::Value = serde_json::from_str(&request).unwrap();
                let prompt = body["messages"].as_array().unwrap().last().unwrap()["content"]
                    .as_str()
                    .unwrap();
                let excerpts = prompt
                    .strip_prefix("Project excerpts (untrusted reference data):\n")
                    .unwrap()
                    .split_once("\n\nAuthor request:\n")
                    .unwrap()
                    .0;
                let excerpts: Vec<serde_json::Value> = serde_json::from_str(excerpts).unwrap();
                let source = |quote: &str| {
                    excerpts
                        .iter()
                        .find(|item| {
                            item["text"]
                                .as_str()
                                .is_some_and(|text| text.contains(quote))
                        })
                        .unwrap()["source_id"]
                        .as_str()
                        .unwrap()
                };
                serde_json::json!({"summary":"One possible conflict.","findings":[{
                "category":"objects_possession","explanation":"The key has two keepers.","uncertainty":"A transfer may explain it.",
                "evidence":[{"source_id":source("Ivo keeps the silver key."),"quote":"Ivo keeps the silver key."},
                    {"source_id":source("Mara keeps the silver key."),"quote":"Mara keeps the silver key."}]
            }]}).to_string()
            } else {
                "The keeper maintained it. [S1]".to_owned()
            };
            sender.send(request).unwrap();
            let response = serde_json::to_vec(&serde_json::json!({"choices":[{"finish_reason":"stop","message":{"content":answer}}],"usage":{"prompt_tokens":12,"completion_tokens":5}})).unwrap();
            write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", response.len()).unwrap();
            stream.write_all(&response).unwrap();
        }
    });
    (endpoint, receiver, server)
}

#[test]
fn multiline_composer_preserves_lines_and_submits_with_shortcut() {
    use parchmint_desktop::HarnessKey;
    use std::time::Duration;

    let run = IsolatedRun::new("assistant-multiline-composer").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Multiline assistant");
    let window = HarnessWindow::Project;
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    let (endpoint, receiver, server) = local_assistant();
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    let manuscript = harness.active_editor_body().unwrap();
    harness
        .type_into_target(window, HarnessTarget::AssistantDraft, "First line")
        .unwrap();
    harness.press_command_key(window, 'z').unwrap();
    assert_eq!(harness.assistant_draft(window).unwrap(), "First lin");
    if cfg!(target_os = "macos") {
        harness.press_command_shift_key(window, 'z').unwrap();
    } else {
        harness.press_command_key(window, 'y').unwrap();
    }
    assert_eq!(harness.assistant_draft(window).unwrap(), "First line");
    assert_eq!(harness.active_editor_body().unwrap(), manuscript);
    harness.press_key(window, HarnessKey::ArrowLeft).unwrap();
    harness.press_key(window, HarnessKey::ArrowLeft).unwrap();
    harness.type_focused(window, "X").unwrap();
    harness.press_command_key(window, 'z').unwrap();
    harness.type_focused(window, "Q").unwrap();
    assert_eq!(harness.assistant_draft(window).unwrap(), "First liQne");
    harness.press_command_key(window, 'z').unwrap();
    harness.press_key(window, HarnessKey::End).unwrap();
    harness.press_key(window, HarnessKey::Enter).unwrap();
    harness.type_focused(window, "Second line").unwrap();
    assert_eq!(
        harness.assistant_draft(window).unwrap(),
        "First line\nSecond line"
    );
    assert!(
        harness
            .target_is_focused(window, HarnessTarget::AssistantDraft)
            .unwrap()
    );
    assert!(receiver.try_recv().is_err());
    harness.defer_next_assistant_request().unwrap();
    harness.press_key(window, HarnessKey::PrimaryEnter).unwrap();
    let payload = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(payload.contains("First line\\nSecond line"), "{payload}");
    server.join().unwrap();
    harness
        .replace_target(window, HarnessTarget::AssistantDraft, "Next question")
        .unwrap();
    harness.press_key(window, HarnessKey::PrimaryEnter).unwrap();
    assert!(harness.contains_text(window, "Stop").unwrap());
    assert_eq!(harness.assistant_draft(window).unwrap(), "Next question");
    harness.complete_deferred_tasks().unwrap();
    assert_eq!(harness.assistant_draft(window).unwrap(), "Next question");
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn colon_note_request_sends_the_requested_current_document_note() {
    use parchmint_desktop::{EditorPane, HarnessKey};
    use std::time::Duration;
    let run = IsolatedRun::new("assistant-colon-notes").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Note request");
    let window = HarnessWindow::Project;
    harness
        .type_into_target(
            window,
            HarnessTarget::EditorPrimary,
            "Marin reaches the harbor.",
        )
        .unwrap();
    harness
        .select_editor_text(window, EditorPane::Primary, "Marin")
        .unwrap();
    harness
        .right_click_target_at(window, HarnessTarget::EditorPrimary, (0.5, 0.5))
        .unwrap();
    harness.click_text(window, "Add note").unwrap();
    harness.type_focused(window, "Private harbor note").unwrap();
    harness.press_key(window, HarnessKey::Enter).unwrap();
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    let (endpoint, receiver, server) = local_assistant();
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .type_into_target(
            window,
            HarnessTarget::AssistantDraft,
            "Read my notes: what do they say about the key?",
        )
        .unwrap();
    harness.click_text(window, "Send").unwrap();
    let sent = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(sent.contains("Private harbor note"));
    server.join().unwrap();
    for _ in 0..100 {
        if harness
            .contains_text(window, "The keeper maintained it.")
            .unwrap()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    harness
        .type_into_target(
            window,
            HarnessTarget::AssistantDraft,
            "Read notes on this scene but avoid confidential notes",
        )
        .unwrap();
    harness.click_text(window, "Send").unwrap();
    assert!(
        harness
            .contains_text(
                window,
                "Clarify which notes to use. Your request is retained."
            )
            .unwrap()
    );
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn explicitly_selected_notes_are_sent_once_for_a_natural_question() {
    use parchmint_desktop::{EditorPane, HarnessKey};
    use std::time::Duration;
    let run = IsolatedRun::new("assistant-explicit-notes").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "One-request notes");
    let window = HarnessWindow::Project;
    harness
        .type_into_target(
            window,
            HarnessTarget::EditorPrimary,
            "Marin reaches the harbor.",
        )
        .unwrap();
    harness
        .select_editor_text(window, EditorPane::Primary, "Marin")
        .unwrap();
    harness
        .right_click_target_at(window, HarnessTarget::EditorPrimary, (0.5, 0.5))
        .unwrap();
    harness.click_text(window, "Add note").unwrap();
    harness.type_focused(window, "Private harbor note").unwrap();
    harness.press_key(window, HarnessKey::Enter).unwrap();
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    let (endpoint, receiver, server) = local_assistant_with_check_requests(false, 2);
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .type_into_target(
            window,
            HarnessTarget::AssistantDraft,
            "What should Marin remember?",
        )
        .unwrap();
    harness
        .click_text(window, "Project context · Automatic")
        .unwrap();
    harness.click_text(window, "Current").unwrap();
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::Path::new(&root).join("assistant-note-scope"),
            )
            .unwrap();
    }
    harness.click_text(window, "Send").unwrap();
    let first = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(first.contains("Private harbor note"));
    for _ in 0..100 {
        if harness
            .contains_text(window, "The keeper maintained it.")
            .unwrap()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    harness.click_text(window, "Hide project context").unwrap();
    assert!(
        harness
            .contains_text(window, "Project context · Automatic")
            .unwrap()
    );
    harness
        .type_into_target(window, HarnessTarget::AssistantDraft, "What happens next?")
        .unwrap();
    harness.click_text(window, "Send").unwrap();
    let second = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(!second.contains("Private harbor note"));
    server.join().unwrap();
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn streamed_answer_is_visible_only_in_its_conversation_and_saved_after_completion() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        time::Duration,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let (started, first_chunk) = mpsc::channel();
    let (release, wait) = mpsc::channel::<()>();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        let header_end = loop {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
        let length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            })
            .unwrap();
        while bytes.len() < header_end + length {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
        }
        let body: serde_json::Value =
            serde_json::from_slice(&bytes[header_end..header_end + length]).unwrap();
        assert_eq!(body["stream"], true);
        let first = format!(
            "data: {}\n\n",
            serde_json::json!({"id":"test","choices":[{"index":0,"delta":{"content":"A partial answer"},"finish_reason":null}]})
        );
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:X}\r\n{}\r\n", first.len(), first).unwrap();
        socket.flush().unwrap();
        started.send(()).unwrap();
        wait.recv_timeout(Duration::from_secs(60)).unwrap();
        let last = format!(
            "data: {}\n\ndata: [DONE]\n\n",
            serde_json::json!({"id":"test","choices":[{"index":0,"delta":{"content":" completed."},"finish_reason":"stop"}]})
        );
        write!(socket, "{:X}\r\n{}\r\n0\r\n\r\n", last.len(), last).unwrap();
        socket.flush().unwrap();
    });

    let run = IsolatedRun::new("assistant-streamed-answer").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Streaming conversations");
    let window = HarnessWindow::Project;
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-stream-settings"),
            )
            .unwrap();
    }
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Stream responses").unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .type_into_target(
            window,
            HarnessTarget::AssistantDraft,
            "What does Mara know?",
        )
        .unwrap();
    harness.defer_next_assistant_request().unwrap();
    harness.click_text(window, "Send").unwrap();
    first_chunk.recv_timeout(Duration::from_secs(15)).unwrap();
    let mut visible = false;
    for _ in 0..100 {
        if harness.contains_text(window, "A partial answer").unwrap() {
            visible = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(visible, "streamed text was not displayed before completion");
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-stream-preview"),
            )
            .unwrap();
    }
    harness
        .click_target(window, HarnessTarget::AssistantNewConversation)
        .unwrap();
    assert!(!harness.contains_text(window, "A partial answer").unwrap());
    release.send(()).unwrap();
    server.join().unwrap();
    harness.complete_deferred_tasks().unwrap();
    assert!(
        !harness
            .contains_text(window, "No cost estimate or local spending limit.")
            .unwrap()
    );
    harness
        .click_target(window, HarnessTarget::AssistantConversationPicker)
        .unwrap();
    harness.click_text(window, "What does Mara know?").unwrap();
    assert!(
        harness
            .contains_text(window, "No cost estimate or local spending limit.")
            .unwrap()
    );
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
    let history_dir = run.root().join("data").join("assistant-conversations");
    let history_path = fs::read_dir(history_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let history: serde_json::Value =
        serde_json::from_slice(&fs::read(history_path).unwrap()).unwrap();
    let threads = history["state"]["threads"].as_array().unwrap();
    assert!(
        threads.iter().any(
            |thread| thread["turns"].as_array().is_some_and(|turns| turns
                .iter()
                .any(|turn| turn["result"]["response"]["text"] == "A partial answer completed."))
        )
    );
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    if !reopened.contains_text(window, "Ask").unwrap() {
        reopened
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    reopened.click_text(window, "Ask").unwrap();
    assert!(
        reopened
            .contains_text(window, "No cost estimate or local spending limit.")
            .unwrap()
    );
    reopened.close(window).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn stopping_a_stream_closes_the_connection_without_saving_partial_text() {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        sync::mpsc,
        time::Duration,
    };

    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let endpoint = format!("http://{}/v1", listener.local_addr().unwrap());
    let (started, first_chunk) = mpsc::channel();
    let (closed, connection_closed) = mpsc::channel();
    let server = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        socket
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let mut bytes = Vec::new();
        let mut buffer = [0; 4096];
        let header_end = loop {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
            if let Some(index) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
                break index + 4;
            }
        };
        let headers = std::str::from_utf8(&bytes[..header_end]).unwrap();
        let length = headers
            .lines()
            .find_map(|line| {
                line.split_once(':')
                    .filter(|(name, _)| name.eq_ignore_ascii_case("content-length"))
                    .and_then(|(_, value)| value.trim().parse::<usize>().ok())
            })
            .unwrap();
        while bytes.len() < header_end + length {
            let count = socket.read(&mut buffer).unwrap();
            assert!(count > 0);
            bytes.extend_from_slice(&buffer[..count]);
        }
        let first = format!(
            "data: {}\n\n",
            serde_json::json!({"id":"test","choices":[{"index":0,"delta":{"content":"Unfinished stream"},"finish_reason":null}]})
        );
        write!(socket, "HTTP/1.1 200 OK\r\nContent-Type: text/event-stream\r\nTransfer-Encoding: chunked\r\n\r\n{:X}\r\n{}\r\n", first.len(), first).unwrap();
        socket.flush().unwrap();
        started.send(()).unwrap();
        let result = socket.read(&mut buffer);
        closed
            .send(
                matches!(result, Ok(0))
                    || result.as_ref().is_err_and(|error| {
                        matches!(
                            error.kind(),
                            std::io::ErrorKind::ConnectionReset
                                | std::io::ErrorKind::ConnectionAborted
                        )
                    }),
            )
            .unwrap();
    });

    let run = IsolatedRun::new("assistant-stop-stream").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Stopped stream");
    let window = HarnessWindow::Project;
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Stream responses").unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .type_into_target(window, HarnessTarget::AssistantDraft, "A request to stop")
        .unwrap();
    harness.defer_next_assistant_request().unwrap();
    harness.click_text(window, "Send").unwrap();
    first_chunk.recv_timeout(Duration::from_secs(15)).unwrap();
    for _ in 0..100 {
        if harness.contains_text(window, "Unfinished stream").unwrap() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(harness.contains_text(window, "Unfinished stream").unwrap());
    harness.click_text(window, "Stop").unwrap();
    assert!(
        connection_closed
            .recv_timeout(Duration::from_secs(5))
            .unwrap()
    );
    harness.complete_deferred_tasks().unwrap();
    assert!(!harness.contains_text(window, "Unfinished stream").unwrap());
    server.join().unwrap();
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
    let history_dir = run.root().join("data").join("assistant-conversations");
    let history_path = fs::read_dir(history_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let history: serde_json::Value =
        serde_json::from_slice(&fs::read(history_path).unwrap()).unwrap();
    let threads = history["state"]["threads"].as_array().unwrap();
    assert!(
        threads
            .iter()
            .all(|thread| thread["turns"].as_array().is_none_or(Vec::is_empty))
    );
    assert!(!history.to_string().contains("Unfinished stream"));
}

#[test]
fn check_edit_proposal_applies_once_and_undoes_as_one_change() {
    use std::time::Duration;
    let run = IsolatedRun::new("assistant-check-edit").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Continuity");
    let window = HarnessWindow::Project;
    create_document(&harness, "Lore", "Keeper");
    harness
        .type_into_target(
            window,
            HarnessTarget::EditorPrimary,
            "Mara keeps the silver key.",
        )
        .unwrap();
    create_document(&harness, "Manuscript", "Draft");
    harness
        .type_into_target(
            window,
            HarnessTarget::EditorPrimary,
            "Ivo keeps the silver key.",
        )
        .unwrap();
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    let (endpoint, receiver, server) = local_assistant_with_check_requests(true, 4);
    harness.click_text(window, "Check").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantSubmit)
        .unwrap();
    receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    for _ in 0..100 {
        if harness
            .contains_text(window, "› Objects · The key has two keepers.")
            .unwrap()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    harness
        .click_text(window, "› Objects · The key has two keepers.")
        .unwrap();
    harness.click_text(window, "Propose edit").unwrap();
    let request = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(request.contains("Suggest a replacement"));
    for _ in 0..100 {
        if harness
            .contains_text(window, "Ivo once carried the silver key.")
            .unwrap()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        harness
            .contains_text(window, "Ivo once carried the silver key.")
            .unwrap()
    );
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-edit-proposal"),
            )
            .unwrap();
    }
    harness
        .type_into_target(window, HarnessTarget::EditorPrimary, " extra")
        .unwrap();
    let changed = harness.active_editor_body().unwrap();
    harness.click_text(window, "Apply").unwrap();
    assert_eq!(harness.active_editor_body().unwrap(), changed);
    harness.click_text(window, "Discard").unwrap();
    harness
        .click_target(window, HarnessTarget::EditorPrimary)
        .unwrap();
    harness.press_command_key(window, 'z').unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Ivo keeps the silver key.")
    );
    harness
        .click_target(window, HarnessTarget::AssistantNewConversation)
        .unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantSubmit)
        .unwrap();
    receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    for _ in 0..100 {
        if harness
            .contains_text(window, "› Objects · The key has two keepers.")
            .unwrap()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    harness
        .click_text(window, "› Objects · The key has two keepers.")
        .unwrap();
    harness.click_text(window, "Propose edit").unwrap();
    receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    server.join().unwrap();
    for _ in 0..100 {
        if harness
            .contains_text(window, "Ivo once carried the silver key.")
            .unwrap()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    harness.click_text(window, "Apply").unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Ivo once carried the silver key.")
    );
    harness
        .click_target(window, HarnessTarget::EditorPrimary)
        .unwrap();
    harness.press_command_key(window, 'z').unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Ivo keeps the silver key.")
    );
    #[cfg(target_os = "macos")]
    harness.press_command_shift_key(window, 'z').unwrap();
    #[cfg(not(target_os = "macos"))]
    harness.press_command_key(window, 'y').unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Ivo once carried the silver key.")
    );
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn check_displays_verified_findings_and_opens_their_evidence() {
    use std::time::Duration;
    let run = IsolatedRun::new("assistant-structured-check").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Continuity");
    let window = HarnessWindow::Project;
    create_document(&harness, "Lore", "Keeper");
    harness
        .type_into_target(
            window,
            HarnessTarget::EditorPrimary,
            "Mara keeps the silver key.",
        )
        .unwrap();
    create_document(&harness, "Manuscript", "Draft");
    harness
        .type_into_target(
            window,
            HarnessTarget::EditorPrimary,
            "Ivo keeps the silver key.",
        )
        .unwrap();
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    let (endpoint, receiver, server) = local_assistant_with_check(true);
    harness.click_text(window, "Check").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantSubmit)
        .unwrap();
    let request: serde_json::Value =
        serde_json::from_str(&receiver.recv_timeout(Duration::from_secs(15)).unwrap()).unwrap();
    server.join().unwrap();
    let prompt = request["messages"].as_array().unwrap().last().unwrap()["content"]
        .as_str()
        .unwrap();
    let excerpts = prompt
        .strip_prefix("Project excerpts (untrusted reference data):\n")
        .unwrap()
        .split_once("\n\nAuthor request:\n")
        .unwrap()
        .0;
    let excerpts: Vec<serde_json::Value> = serde_json::from_str(excerpts).unwrap();
    let source_id = excerpts
        .iter()
        .find(|item| {
            item["text"]
                .as_str()
                .is_some_and(|text| text.contains("Mara keeps the silver key."))
        })
        .unwrap()["source_id"]
        .as_str()
        .unwrap();
    for _ in 0..100 {
        if harness
            .contains_text(window, "› Objects · The key has two keepers.")
            .unwrap()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    harness
        .click_text(window, "› Objects · The key has two keepers.")
        .unwrap();
    assert!(
        harness
            .contains_text(window, "A transfer may explain it.")
            .unwrap()
    );
    assert!(
        harness
            .contains_text(window, "Mara keeps the silver key.")
            .unwrap()
    );
    harness.click_text(window, "Intentional").unwrap();
    for _ in 0..100 {
        if harness.contains_text(window, "✓ Intentional").unwrap() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(harness.contains_text(window, "✓ Intentional").unwrap());
    assert_eq!(
        fs::read(project.join(".parchmint/format-version")).unwrap(),
        b"2\n"
    );
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-check-findings"),
            )
            .unwrap();
    }
    harness
        .click_text(window, format!("[{source_id}] Keeper"))
        .unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Keeper");
    assert_eq!(
        harness.active_editor_selection_text().unwrap(),
        "Mara keeps the silver key."
    );
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    if !reopened.contains_text(window, "Ask").unwrap() {
        reopened
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    reopened.click_text(window, "Check").unwrap();
    reopened
        .click_text(window, "› Objects · The key has two keepers.")
        .unwrap();
    assert!(reopened.contains_text(window, "✓ Intentional").unwrap());
    reopened
        .click_text(window, format!("[{source_id}] Keeper"))
        .unwrap();
    reopened
        .type_focused(window, "Mara once held the key.")
        .unwrap();
    assert!(
        reopened
            .contains_text(window, "Intentional · Needs recheck")
            .unwrap()
    );
    reopened.close(window).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn lore_is_authorable_and_missing_key_stays_in_the_assistant() {
    let run = IsolatedRun::new("assistant-lore").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Assistant novel");
    let window = HarnessWindow::Project;
    assert!(
        !harness
            .target_is_visible(window, HarnessTarget::ToggleCompanion)
            .unwrap()
    );
    assert!(
        !harness
            .target_is_visible(window, HarnessTarget::EditorCompanion)
            .unwrap()
    );

    create_document(&harness, "Lore", "Harbor rules");
    let text = "The harbor closes at sunset.";
    harness
        .type_into_target(window, HarnessTarget::EditorPrimary, text)
        .unwrap();
    harness.elapse_autosave_idle().unwrap();
    assert!(fs::read_dir(project.join("lore")).unwrap().any(|entry| {
        let path = entry.unwrap().path();
        fs::read_to_string(path).is_ok_and(|body| body.contains(text))
    }));

    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    harness.click_text(window, "Ask").unwrap();
    assert!(
        harness
            .contains_text(window, "No AI API key has been set up.")
            .unwrap()
    );
    assert!(harness.contains_text(window, "Open Settings").unwrap());
    harness.click_text(window, "Notes").unwrap();
    assert!(harness.contains_text(window, "No notes yet").unwrap());
    assert!(
        !harness
            .contains_text(window, "No AI API key has been set up.")
            .unwrap()
    );
    harness.click_text(window, "Ask").unwrap();
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-unconfigured"),
            )
            .unwrap();
    }
    assert!(
        !harness
            .contains_text(window, "AI assistant settings")
            .unwrap()
    );
    harness.click_text(window, "Open Settings").unwrap();
    assert!(harness.contains_text(window, "OpenAI-compatible").unwrap());
    assert!(harness.contains_text(window, "Anthropic").unwrap());
    assert!(harness.contains_text(window, "Model").unwrap());
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-settings"),
            )
            .unwrap();
    }
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Anthropic").unwrap();
    assert!(harness.contains_text(window, "claude-sonnet-4-6").unwrap());
    assert!(harness.contains_text(window, "API key").unwrap());
    assert!(
        !harness
            .contains_text(window, "Local endpoint does not require an API key")
            .unwrap()
    );
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    assert!(
        harness
            .contains_text(window, "No AI API key has been set up.")
            .unwrap()
    );
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Settings))
            .unwrap();
        harness.click_text(window, "Appearance").unwrap();
        harness.click_text(window, "Dark").unwrap();
        harness
            .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
            .unwrap();
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-dark"),
            )
            .unwrap();
    }
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn assistant_draft_survives_project_close_and_reopen() {
    let run = IsolatedRun::new("assistant-draft-reopen").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Assistant drafts");
    let window = HarnessWindow::Project;
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", "http://127.0.0.1:8181/v1")
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .type_into_target(
            window,
            HarnessTarget::AssistantDraft,
            "Remember the lighthouse rule",
        )
        .unwrap();
    harness
        .click_text(window, "Project context · Automatic")
        .unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantScope)
        .unwrap();
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-scope-menu"),
            )
            .unwrap();
    }
    harness
        .click_target_offset(window, HarnessTarget::AssistantScope, (0.5, -0.5))
        .unwrap();
    assert!(harness.contains_text(window, "History excluded").unwrap());
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-context-controls"),
            )
            .unwrap();
    }
    harness.click_text(window, "Notes").unwrap();
    harness.click_text(window, "Ask").unwrap();
    harness.close(window).unwrap();
    harness.shutdown().unwrap();

    let history_path = fs::read_dir(run.root().join("data").join("assistant-conversations"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let history: serde_json::Value =
        serde_json::from_slice(&fs::read(history_path).unwrap()).unwrap();
    let ask = history["state"]["threads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|thread| thread["mode"] == "Ask" && thread["active"] == true)
        .unwrap();
    assert!(ask["context_scope"].is_string());
    assert_eq!(ask["selection_only"], false);

    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    if !reopened.contains_text(window, "Ask").unwrap() {
        reopened
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    reopened.click_text(window, "Ask").unwrap();
    reopened
        .click_text(window, "Project context · Customized")
        .unwrap();
    assert!(reopened.contains_text(window, "History excluded").unwrap());
    assert_eq!(
        reopened.assistant_draft(window).unwrap(),
        "Remember the lighthouse rule"
    );
    reopened.close(window).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn conversations_can_be_named_switched_and_reopened() {
    let run = IsolatedRun::new("assistant-conversations").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Conversation novel");
    let window = HarnessWindow::Project;
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", "http://127.0.0.1:8181/v1")
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .type_into_target(window, HarnessTarget::AssistantDraft, "First private draft")
        .unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantConversationMenu)
        .unwrap();
    harness.click_text(window, "Rename").unwrap();
    harness
        .replace_target(
            window,
            HarnessTarget::AssistantConversationName,
            "Characters",
        )
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantNewConversation)
        .unwrap();
    harness
        .type_into_target(
            window,
            HarnessTarget::AssistantDraft,
            "Second private draft",
        )
        .unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantConversationMenu)
        .unwrap();
    harness.click_text(window, "Rename").unwrap();
    harness
        .replace_target(window, HarnessTarget::AssistantConversationName, "Timeline")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantConversationPicker)
        .unwrap();
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-conversation-list"),
            )
            .unwrap();
    }
    harness.click_text(window, "Characters").unwrap();
    assert_eq!(
        harness.assistant_draft(window).unwrap(),
        "First private draft"
    );
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-conversations"),
            )
            .unwrap();
    }
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    if !reopened.contains_text(window, "Ask").unwrap() {
        reopened
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    reopened.click_text(window, "Ask").unwrap();
    assert_eq!(
        reopened.assistant_draft(window).unwrap(),
        "First private draft"
    );
    reopened
        .click_target(window, HarnessTarget::AssistantConversationPicker)
        .unwrap();
    reopened.click_text(window, "Timeline").unwrap();
    assert_eq!(
        reopened.assistant_draft(window).unwrap(),
        "Second private draft"
    );
    reopened
        .click_target(window, HarnessTarget::AssistantConversationMenu)
        .unwrap();
    reopened.click_text(window, "Delete").unwrap();
    assert!(
        reopened
            .contains_text(window, "Delete conversation?")
            .unwrap()
    );
    reopened.click_text(window, "Delete").unwrap();
    assert_eq!(
        reopened.assistant_draft(window).unwrap(),
        "First private draft"
    );
    reopened.close(window).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn a_pending_answer_returns_to_its_own_conversation() {
    use std::time::Duration;
    let run = IsolatedRun::new("assistant-concurrent-conversations").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Concurrent conversations");
    let window = HarnessWindow::Project;
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    let (endpoint, receiver, server) = local_assistant();
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .type_into_target(
            window,
            HarnessTarget::AssistantDraft,
            "Who owns the beacon?",
        )
        .unwrap();
    harness.hold_completions().unwrap();
    harness.click_text(window, "Send").unwrap();
    receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    server.join().unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantNewConversation)
        .unwrap();
    harness
        .type_into_target(window, HarnessTarget::AssistantDraft, "Another draft")
        .unwrap();
    harness.release_completions(false).unwrap();
    assert_eq!(harness.assistant_draft(window).unwrap(), "Another draft");
    assert!(
        !harness
            .contains_text(window, "Unverified citations")
            .unwrap()
    );
    harness
        .click_target(window, HarnessTarget::AssistantConversationPicker)
        .unwrap();
    harness.click_text(window, "Who owns the beacon?").unwrap();
    for _ in 0..100 {
        if harness
            .contains_text(window, "Unverified citations")
            .unwrap()
        {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        harness
            .contains_text(window, "Unverified citations")
            .unwrap()
    );
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn new_assistant_replies_remain_visible_in_a_long_conversation() {
    use std::time::Duration;
    let run = IsolatedRun::new("assistant-long-conversation").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Long conversation");
    let window = HarnessWindow::Project;
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    let (endpoint, receiver, server) = local_assistant_with_check_requests(false, 6);
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    for index in 0..6 {
        let question = format!(
            "Question {index}: What happened to the harbor lantern before the storm arrived?"
        );
        harness
            .type_into_target(window, HarnessTarget::AssistantDraft, &question)
            .unwrap();
        harness.click_text(window, "Send").unwrap();
        receiver.recv_timeout(Duration::from_secs(15)).unwrap();
        for _ in 0..100 {
            if !harness.contains_text(window, "Stop").unwrap() {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
        assert!(!harness.contains_text(window, "Stop").unwrap());
    }
    assert!(
        harness
            .text_is_visible(
                window,
                "Question 5: What happened to the harbor lantern before the storm arrived?"
            )
            .unwrap()
    );
    server.join().unwrap();
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn selection_only_sends_the_live_editor_selection() {
    use parchmint_desktop::EditorPane;
    use std::time::Duration;
    let run = IsolatedRun::new("assistant-selection-only").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Selection context");
    let window = HarnessWindow::Project;
    harness
        .type_into_target(
            window,
            HarnessTarget::EditorPrimary,
            "Private before. The saffron beacon glows. Private after.",
        )
        .unwrap();
    harness.elapse_autosave_idle().unwrap();
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    let (endpoint, receiver, server) = local_assistant();
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .click_text(window, "Project context · Automatic")
        .unwrap();
    harness
        .click_target(window, HarnessTarget::AssistantScope)
        .unwrap();
    harness
        .click_target_offset(window, HarnessTarget::AssistantScope, (0.5, -1.5))
        .unwrap();
    harness
        .type_into_target(
            window,
            HarnessTarget::AssistantDraft,
            "What does the selection say?",
        )
        .unwrap();
    harness.click_text(window, "Send").unwrap();
    assert!(
        harness
            .contains_text(
                window,
                "Select up to 32 KiB of text in an open document before sending"
            )
            .unwrap()
    );
    assert!(receiver.try_recv().is_err());
    harness
        .select_editor_text(window, EditorPane::Primary, "The saffron beacon glows.")
        .unwrap();
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-selection-only"),
            )
            .unwrap();
    }
    harness.click_text(window, "Send").unwrap();
    let payload = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(payload.contains("The saffron beacon glows."));
    assert!(!payload.contains("Private before"));
    assert!(!payload.contains("Private after"));
    server.join().unwrap();
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn assistant_citation_opens_the_exact_authored_passage() {
    use parchmint_desktop::{EditorPane, HarnessKey};
    use std::time::Duration;
    let run = IsolatedRun::new("assistant-passage-navigation").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Passage navigation");
    let window = HarnessWindow::Project;
    create_document(&harness, "Lore", "Keeper");
    let source = "The silver bell belongs to Mara.";
    harness
        .click_target(window, HarnessTarget::EditorPrimary)
        .unwrap();
    let html = format!(
        "{}<p>{source}</p>",
        "<p>Filler paragraph with ordinary details.</p>".repeat(80)
    );
    harness.seed_clipboard(Some(source), Some(&html));
    harness.press_command_key(window, 'v').unwrap();
    assert!(harness.active_editor_body().unwrap().contains(source));
    harness
        .scroll_target_by(window, HarnessTarget::EditorPrimary, 100_000.0)
        .unwrap();
    harness.elapse_autosave_idle().unwrap();
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    let (endpoint, receiver, server) = local_assistant();
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", &endpoint)
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .type_into_target(
            window,
            HarnessTarget::AssistantDraft,
            "Who owns the silver bell?",
        )
        .unwrap();
    harness.click_text(window, "Send").unwrap();
    assert!(
        receiver
            .recv_timeout(Duration::from_secs(15))
            .unwrap()
            .contains(source)
    );
    server.join().unwrap();
    for _ in 0..100 {
        if harness.contains_text(window, "[S1] Keeper").unwrap() {
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    if !harness.contains_text(window, "[S1] Keeper").unwrap() {
        if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
            harness
                .snapshot(
                    window,
                    std::path::PathBuf::from(root).join("assistant-citation-missing"),
                )
                .unwrap();
        }
        panic!("the current citation label is absent");
    }
    harness
        .select_editor_text(window, EditorPane::Primary, "Mara")
        .unwrap();
    create_document(&harness, "Manuscript", "Draft");
    harness
        .type_into_target(window, HarnessTarget::EditorPrimary, "Writing in progress.")
        .unwrap();
    harness
        .select_editor_text(window, EditorPane::Primary, "progress")
        .unwrap();
    harness
        .click_target(window, HarnessTarget::ToggleExplorer)
        .unwrap();
    harness.hold_completions().unwrap();
    harness.click_text(window, "[S1] Keeper").unwrap();
    harness.click_text(window, "Draft").unwrap();
    harness.click_text(window, "Keeper").unwrap();
    harness.release_completions(true).unwrap();
    assert_ne!(harness.active_editor_selection_text().unwrap(), source);
    harness.hold_completions().unwrap();
    harness.click_text(window, "[S1] Keeper").unwrap();
    harness
        .select_editor_text(window, EditorPane::Primary, "silver bell")
        .unwrap();
    harness.release_completions(true).unwrap();
    assert_eq!(
        harness.active_editor_selection_text().unwrap(),
        "silver bell"
    );
    harness.click_text(window, "[S1] Keeper").unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Keeper");
    assert_eq!(harness.active_editor_selection_text().unwrap(), source);
    assert!(harness.active_editor_selection_is_revealed().unwrap());
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("assistant-passage-navigation"),
            )
            .unwrap();
    }
    harness.hold_completions().unwrap();
    harness.click_text(window, "← Back to writing").unwrap();
    harness.click_text(window, "Keeper").unwrap();
    harness.release_completions(true).unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Keeper");
    assert!(harness.contains_text(window, "← Back to writing").unwrap());
    harness.click_text(window, "← Back to writing").unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Draft");
    assert_eq!(harness.active_editor_selection_text().unwrap(), "progress");
    assert!(!harness.contains_text(window, "← Back to writing").unwrap());
    harness.click_text(window, "Keeper").unwrap();
    harness
        .select_editor_text(window, EditorPane::Primary, "Mara")
        .unwrap();
    harness.click_text(window, "[S1] Keeper").unwrap();
    assert_eq!(harness.active_editor_selection_text().unwrap(), source);
    harness.click_text(window, "← Back to writing").unwrap();
    assert_eq!(harness.active_editor_selection_text().unwrap(), "Mara");
    harness.click_text(window, "Draft").unwrap();
    harness
        .select_editor_text(window, EditorPane::Primary, "progress")
        .unwrap();
    harness.click_text(window, "[S1] Keeper").unwrap();
    harness.click_text(window, "Draft").unwrap();
    harness
        .select_editor_text(window, EditorPane::Primary, "progress")
        .unwrap();
    harness.type_focused(window, "updated").unwrap();
    harness.click_text(window, "Keeper").unwrap();
    harness.click_text(window, "← Back to writing").unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Draft");
    assert!(harness.active_editor_body().unwrap().contains("updated"));
    harness
        .click_target(window, HarnessTarget::NewTab(EditorPane::Primary))
        .unwrap();
    let scratch = harness
        .active_editor_document_id(EditorPane::Primary)
        .unwrap();
    harness.click_text(window, "[S1] Keeper").unwrap();
    harness.click_text(window, "← Back to writing").unwrap();
    assert_eq!(
        harness
            .active_editor_document_id(EditorPane::Primary)
            .unwrap(),
        scratch
    );
    harness.hold_completions().unwrap();
    harness
        .type_into_target(
            window,
            HarnessTarget::EditorPrimary,
            "Unfinished scratch writing.",
        )
        .unwrap();
    harness.click_text(window, "[S1] Keeper").unwrap();
    harness.release_completions(false).unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Keeper");
    assert!(harness.contains_text(window, "← Back to writing").unwrap());
    harness.click_text(window, "← Back to writing").unwrap();
    assert!(
        harness
            .active_editor_body()
            .unwrap()
            .contains("Unfinished scratch writing.")
    );
    harness
        .click_target(window, HarnessTarget::NewTab(EditorPane::Primary))
        .unwrap();
    harness.hold_completions().unwrap();
    harness
        .click_target(window, HarnessTarget::BreakMenu)
        .unwrap();
    harness.press_key(window, HarnessKey::ArrowUp).unwrap();
    harness.press_key(window, HarnessKey::Enter).unwrap();
    harness.click_text(window, "[S1] Keeper").unwrap();
    harness.release_completions(false).unwrap();
    assert_eq!(harness.active_editor_tab_title().unwrap(), "Keeper");
    harness.click_text(window, "← Back to writing").unwrap();
    let scene = harness.active_editor_body().unwrap();
    assert!(scene.contains("data-kind=\"scene-break\""), "{scene}");
    harness
        .click_target(window, HarnessTarget::NewTab(EditorPane::Primary))
        .unwrap();
    harness.hold_completions().unwrap();
    harness
        .type_into_target(window, HarnessTarget::EditorPrimary, "Before ")
        .unwrap();
    harness
        .click_target(window, HarnessTarget::BreakMenu)
        .unwrap();
    harness.press_key(window, HarnessKey::ArrowUp).unwrap();
    harness.press_key(window, HarnessKey::Enter).unwrap();
    harness
        .type_into_target(window, HarnessTarget::EditorPrimary, "after")
        .unwrap();
    harness.click_text(window, "[S1] Keeper").unwrap();
    let warning = harness.release_completions(false).unwrap_err();
    assert!(warning.to_string().contains("Breaks weren't added"));
    let draft_title = harness.active_editor_tab_title().unwrap();
    let notice = format!("{draft_title}: Breaks weren't added. Add them in the editor.");
    let draft = harness.active_editor_body().unwrap();
    assert!(draft.contains("Before after"), "{draft}");
    assert!(!draft.contains("data-kind=\"scene-break\""), "{draft}");
    assert!(harness.contains_text(window, &notice).unwrap());
    harness.close(window).unwrap();
    assert!(harness.has_window(window).unwrap());
    assert!(harness.contains_text(window, "Notifications").unwrap());
    harness.click_text(window, "Clear").unwrap();
    harness.click_text(window, "[S1] Keeper").unwrap();
    harness.type_focused(window, "A changed passage.").unwrap();
    assert!(
        harness
            .contains_text(window, "[S1] Keeper · changed")
            .unwrap()
    );
    harness.close(window).unwrap();
    harness.shutdown().unwrap();
}

#[test]
fn rejected_original_download_keeps_the_preview_editable_and_durable() {
    let run = IsolatedRun::new("research-original-rejected").unwrap();
    let project = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project, "Original research draft");
    let window = HarnessWindow::Project;
    if !harness.contains_text(window, "Ask").unwrap() {
        harness
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    harness.click_text(window, "Ask").unwrap();
    harness.click_text(window, "Open Settings").unwrap();
    harness.click_text(window, "OpenAI-compatible").unwrap();
    harness
        .type_into(window, "https://example.com/v1", "http://127.0.0.1:8181/v1")
        .unwrap();
    harness
        .click_text(window, "Local endpoint does not require an API key")
        .unwrap();
    harness.click_text(window, "Save").unwrap();
    harness
        .click_target(window, HarnessTarget::Ribbon(RibbonDestination::Editor))
        .unwrap();
    harness
        .click_target(window, HarnessTarget::ToggleExplorer)
        .unwrap();
    harness.click_text(window, "Research").unwrap();
    harness.click_text(window, "Add a source link…").unwrap();
    harness
        .type_into(window, "Source title", "Archive original")
        .unwrap();
    harness
        .type_into(window, "https://…", "https://127.0.0.1/source.pdf")
        .unwrap();
    harness.click_text(window, "Original file").unwrap();
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        harness
            .snapshot(
                window,
                std::path::PathBuf::from(root).join("research-original-preview"),
            )
            .unwrap();
    }
    harness.click_text(window, "Save").unwrap();
    assert!(
        harness
            .contains_text(
                window,
                "The source address resolves to a non-public destination."
            )
            .unwrap()
    );
    assert!(harness.contains_text(window, "Archive original").unwrap());
    harness.close(window).unwrap();
    harness.shutdown().unwrap();

    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project)).unwrap();
    if !reopened.contains_text(window, "Ask").unwrap() {
        reopened
            .click_target(window, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    if reopened.contains_text(window, "Explorer").unwrap() {
        reopened
            .click_target(window, HarnessTarget::ToggleExplorer)
            .unwrap();
    }
    reopened.click_text(window, "Research").unwrap();
    assert!(reopened.contains_text(window, "Archive original").unwrap());
    assert!(
        reopened
            .contains_text(window, "https://127.0.0.1/source.pdf")
            .unwrap()
    );
    assert!(!reopened.contains_text(window, "The previous save may have completed. Check Research before trying again. Changes start a new import.").unwrap());
    reopened.close(window).unwrap();
    reopened.shutdown().unwrap();
}

#[test]
fn declared_original_survives_project_open_ordinary_save_and_reopen() {
    use parchmint_domain::{NodeKind, ProjectId, ResearchOriginal, ResearchSourceId};
    use parchmint_project_format::{CanonicalRelativePath, ContentHash, ProjectFormatCodec};
    use std::collections::BTreeMap;
    use std::time::Duration;

    let run = IsolatedRun::new("research-original-reopen").unwrap();
    let project_path = run.root().join("novel.parchmint");
    let harness = create_project(&run, &project_path, "Original research");
    create_document(&harness, "Research", "Archive source");
    harness.close(HarnessWindow::Project).unwrap();
    harness.shutdown().unwrap();

    let codec = ProjectFormatCodec::default();
    let manifest = codec
        .decode_manifest(&fs::read(project_path.join("project.toml")).unwrap())
        .unwrap();
    let (project, previous_paths) = codec
        .decode_domain_project(&manifest, ProjectId::from_bytes([33; 16]))
        .unwrap()
        .unwrap();
    let frontier = codec.decode_persistence_frontier(&manifest).unwrap();
    let source_document = project
        .nodes
        .iter()
        .find_map(|(_, node)| (node.title == "Archive source").then_some(node.kind))
        .and_then(|kind| match kind {
            NodeKind::Document(document) => Some(document),
            _ => None,
        })
        .unwrap();
    let source_id = ResearchSourceId::from_bytes([35; 16]);
    let text_source_id = ResearchSourceId::from_bytes([36; 16]);
    let mut project = project;
    let original = b"original\r\n\0\xffsource bytes".to_vec();
    let text_original = b"The saffron beacon was maintained by the harbor keeper.\r\n".to_vec();
    project.research_originals.insert(
        source_id,
        ResearchOriginal {
            document: source_document,
            filename: "archive.pdf".into(),
            media_type: "application/pdf".into(),
            source_url: Some("https://example.com/archive.pdf".into()),
            retrieved_at_unix_seconds: 123,
            byte_length: original.len() as u64,
            sha256: *ContentHash::of_bytes(&original).as_bytes(),
        },
    );
    project.research_originals.insert(
        text_source_id,
        ResearchOriginal {
            document: source_document,
            filename: "keepers.txt".into(),
            media_type: "text/plain".into(),
            source_url: Some("https://example.com/keepers.txt".into()),
            retrieved_at_unix_seconds: 124,
            byte_length: text_original.len() as u64,
            sha256: *ContentHash::of_bytes(&text_original).as_bytes(),
        },
    );
    let original_path = CanonicalRelativePath::parse(format!(
        "research/originals/{}.bin",
        parchmint_domain::encode_stable_id(source_id.as_bytes())
    ))
    .unwrap();
    let text_path = CanonicalRelativePath::parse(format!(
        "research/originals/{}.bin",
        parchmint_domain::encode_stable_id(text_source_id.as_bytes())
    ))
    .unwrap();
    let mut existing = BTreeMap::from([
        (original_path.clone(), original.clone()),
        (text_path.clone(), text_original.clone()),
    ]);
    for name in ["styles.css", "dictionary.txt"] {
        if let Ok(bytes) = fs::read(project_path.join(name)) {
            existing.insert(CanonicalRelativePath::parse(name).unwrap(), bytes);
        }
    }
    let bodies = previous_paths
        .documents
        .iter()
        .map(|(id, path)| {
            (
                *id,
                fs::read_to_string(project_path.join(path.as_str())).unwrap(),
            )
        })
        .collect::<BTreeMap<_, _>>();
    let encoding = codec
        .encode_domain_project_with_frontier(
            &project,
            &bodies,
            &existing,
            &previous_paths,
            &frontier,
        )
        .unwrap();
    for (path, resource) in &encoding.resources {
        let target = project_path.join(path.as_str());
        fs::create_dir_all(target.parent().unwrap()).unwrap();
        fs::write(target, &resource.bytes).unwrap();
    }

    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project_path)).unwrap();
    assert!(
        reopened
            .contains_text(HarnessWindow::Project, "Research")
            .unwrap()
    );
    if !reopened
        .contains_text(HarnessWindow::Project, "Archive source")
        .unwrap()
    {
        reopened
            .click_text(HarnessWindow::Project, "Research")
            .unwrap();
    }
    reopened
        .click_text(HarnessWindow::Project, "Archive source")
        .unwrap();
    if !reopened
        .contains_text(HarnessWindow::Project, "Ask")
        .unwrap()
    {
        reopened
            .click_target(HarnessWindow::Project, HarnessTarget::ToggleInspector)
            .unwrap();
    }
    reopened
        .click_target(HarnessWindow::Project, HarnessTarget::ToggleExplorer)
        .unwrap();
    reopened
        .click_text(HarnessWindow::Project, "Research")
        .unwrap();
    let exported = run.root().join("exported-source.pdf");
    reopened.set_next_path_selection(&exported);
    reopened
        .click_text(HarnessWindow::Project, "Save original: archive.pdf…")
        .unwrap();
    assert_eq!(fs::read(exported).unwrap(), original);

    // Exercise the real project query and local provider boundary with a
    // loopback test server; no external service or credential is used.
    let (endpoint, receiver, server) = local_assistant();
    reopened.click_text(HarnessWindow::Project, "Ask").unwrap();
    reopened
        .click_text(HarnessWindow::Project, "Open Settings")
        .unwrap();
    reopened
        .click_text(HarnessWindow::Project, "OpenAI-compatible")
        .unwrap();
    reopened
        .type_into(HarnessWindow::Project, "https://example.com/v1", &endpoint)
        .unwrap();
    reopened
        .click_text(
            HarnessWindow::Project,
            "Local endpoint does not require an API key",
        )
        .unwrap();
    reopened.click_text(HarnessWindow::Project, "Save").unwrap();
    reopened
        .click_target(
            HarnessWindow::Project,
            HarnessTarget::Ribbon(RibbonDestination::Editor),
        )
        .unwrap();
    reopened
        .type_into_target(
            HarnessWindow::Project,
            HarnessTarget::AssistantDraft,
            "Who maintained the saffron beacon?",
        )
        .unwrap();
    reopened.click_text(HarnessWindow::Project, "Send").unwrap();
    let payload = receiver.recv_timeout(Duration::from_secs(15)).unwrap();
    assert!(payload.contains("saffron beacon was maintained"));
    assert!(payload.contains("keepers.txt"));
    assert!(!payload.contains("original\\r\\n\\u0000"));
    server.join().unwrap();
    let mut answer_visible = false;
    for _ in 0..100 {
        if reopened
            .contains_text(HarnessWindow::Project, "Unavailable sources")
            .unwrap()
        {
            answer_visible = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    assert!(
        answer_visible,
        "assistant result did not report the unsupported original"
    );
    let disclosure = (1..=48)
        .map(|count| format!("Context ({count})"))
        .find(|label| {
            reopened
                .contains_text(HarnessWindow::Project, label)
                .unwrap()
        })
        .expect("the extracted passage disclosure is visible");
    reopened
        .click_text(HarnessWindow::Project, disclosure)
        .unwrap();
    assert!(
        reopened
            .contains_text(
                HarnessWindow::Project,
                "The saffron beacon was maintained by the harbor keeper."
            )
            .unwrap()
    );
    assert!(
        reopened
            .contains_text(
                HarnessWindow::Project,
                "archive.pdf: this source format is not yet searchable."
            )
            .unwrap()
    );
    if let Some(root) = std::env::var_os("PARCHMINT_REVIEW_ARTIFACTS") {
        reopened
            .snapshot(
                HarnessWindow::Project,
                std::path::PathBuf::from(root).join("assistant-original-evidence"),
            )
            .unwrap();
    }
    assert_eq!(
        fs::read(project_path.join(text_path.as_str())).unwrap(),
        text_original
    );
    reopened
        .click_target(HarnessWindow::Project, HarnessTarget::ToggleExplorer)
        .unwrap();
    create_document(&reopened, "Manuscript", "Chapter after source");
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
    assert_eq!(
        fs::read(project_path.join(original_path.as_str())).unwrap(),
        original
    );

    let reopened =
        DesktopInteractionHarness::launch(run.root(), LaunchRequest::open(&project_path)).unwrap();
    assert!(
        reopened
            .contains_text(HarnessWindow::Project, "Chapter after source")
            .unwrap()
    );
    assert_eq!(
        fs::read(project_path.join(original_path.as_str())).unwrap(),
        original
    );
    reopened.close(HarnessWindow::Project).unwrap();
    reopened.shutdown().unwrap();
}

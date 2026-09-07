use parchmint_diagnostics::{Level, enabled};

#[test]
fn disabled_trace_events_do_not_evaluate_their_fields() {
    let evaluated = std::cell::Cell::new(false);
    parchmint_diagnostics::event!(Level::Trace, "test", "filtered trace", {
        evaluated.set(true);
        &[]
    });
    assert_eq!(
        evaluated.get(),
        cfg!(any(debug_assertions, feature = "capture"))
    );
    assert!(enabled(Level::Warn));
    assert!(enabled(Level::Error));
}

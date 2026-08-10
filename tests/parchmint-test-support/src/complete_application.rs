//! Reusable document-scale fixtures for complete-application integration tests.
//!
//! The normal fixture is deliberately representative rather than a release
//! threshold. The large fixture is the release-gate 250,000-word document.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex},
    time::Duration,
};

/// A representative normal document used by full-application scenarios.
pub const NORMAL_DOCUMENT_WORDS: usize = 2_048;

/// The document size required by the v1 release gate.
pub const LARGE_DOCUMENT_WORDS: usize = 250_000;

/// The number of views opened for one document fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewCount {
    One,
    Two,
}

/// How the fixture assigns its document bodies to the mounted views.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewTopology {
    OneView,
    SameDocumentTwoViews,
    DistinctDocumentsTwoViews,
}

impl ViewTopology {
    /// Returns the number of simultaneously mounted views.
    pub const fn views(self) -> ViewCount {
        match self {
            Self::OneView => ViewCount::One,
            Self::SameDocumentTwoViews | Self::DistinctDocumentsTwoViews => ViewCount::Two,
        }
    }

    /// Returns the number of separate document bodies required by the fixture.
    pub const fn document_count(self) -> usize {
        match self {
            Self::OneView | Self::SameDocumentTwoViews => 1,
            Self::DistinctDocumentsTwoViews => 2,
        }
    }
}

impl ViewCount {
    /// Returns the number of simultaneously mounted views.
    pub const fn value(self) -> usize {
        match self {
            Self::One => 1,
            Self::Two => 2,
        }
    }
}

/// A deterministic body and view topology for integrated desktop scenarios.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteApplicationFixture {
    body: String,
    companion_body: Option<String>,
    topology: ViewTopology,
}

impl CompleteApplicationFixture {
    /// Builds a representative normal-size fixture for the requested topology.
    pub fn normal(views: ViewCount) -> Self {
        Self::normal_with_topology(topology_for(views))
    }

    /// Builds the release-gate large fixture for the requested topology.
    pub fn large(views: ViewCount) -> Self {
        Self::large_with_topology(topology_for(views))
    }

    /// Builds a normal fixture with an explicit document-to-view topology.
    pub fn normal_with_topology(topology: ViewTopology) -> Self {
        Self::with_words(NORMAL_DOCUMENT_WORDS, topology)
    }

    /// Builds a release-gate large fixture with an explicit view topology.
    pub fn large_with_topology(topology: ViewTopology) -> Self {
        Self::with_words(LARGE_DOCUMENT_WORDS, topology)
    }

    /// Returns the canonical document body supplied to the application.
    pub fn body(&self) -> &str {
        &self.body
    }

    /// Returns the companion document body when the two views show different documents.
    pub fn companion_body(&self) -> Option<&str> {
        self.companion_body.as_deref()
    }

    /// Returns the exact number of words in the fixture body.
    pub fn word_count(&self) -> usize {
        self.body.split_whitespace().count()
    }

    /// Returns the required number of independently mounted views.
    pub const fn views(&self) -> ViewCount {
        self.topology.views()
    }

    /// Returns the document-to-view topology this fixture requires.
    pub const fn topology(&self) -> ViewTopology {
        self.topology
    }

    fn with_words(words: usize, topology: ViewTopology) -> Self {
        Self {
            body: "word ".repeat(words).trim_end().to_owned(),
            companion_body: (topology == ViewTopology::DistinctDocumentsTwoViews)
                .then(|| "research ".repeat(words).trim_end().to_owned()),
            topology,
        }
    }
}

const fn topology_for(views: ViewCount) -> ViewTopology {
    match views {
        ViewCount::One => ViewTopology::OneView,
        ViewCount::Two => ViewTopology::SameDocumentTwoViews,
    }
}

/// A named boundary shared by complete-application fault drivers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompleteApplicationBoundary {
    ProjectOpen,
    Save,
    Recovery,
    History,
    Search,
    Spellcheck,
    Export,
}

/// A deterministic fault requested at one integration boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompleteApplicationFault {
    Io,
    Corruption,
    Cancelled,
    WorkerStopped,
}

/// A thread-safe FIFO script. Each requested fault is consumed exactly once.
#[derive(Debug, Clone, Default)]
pub struct CompleteApplicationFaultScript {
    faults: Arc<Mutex<BTreeMap<CompleteApplicationBoundary, VecDeque<CompleteApplicationFault>>>>,
}

impl CompleteApplicationFaultScript {
    pub fn fail_next(
        &self,
        boundary: CompleteApplicationBoundary,
        fault: CompleteApplicationFault,
    ) {
        self.faults
            .lock()
            .expect("complete-application fault script mutex poisoned")
            .entry(boundary)
            .or_default()
            .push_back(fault);
    }

    pub fn take(&self, boundary: CompleteApplicationBoundary) -> Option<CompleteApplicationFault> {
        self.faults
            .lock()
            .expect("complete-application fault script mutex poisoned")
            .get_mut(&boundary)
            .and_then(VecDeque::pop_front)
    }
}

/// An honest operation observation emitted by an integration adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteApplicationObservation {
    pub boundary: CompleteApplicationBoundary,
    pub outcome: String,
}

/// Raw runner measurements with no implied tolerance or release verdict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteApplicationMeasurement {
    pub operation: String,
    pub elapsed: Duration,
    pub resident_bytes: Option<u64>,
    pub platform: String,
    pub hardware_profile: Option<String>,
}

/// Shared collection seam for operation observations and raw measurements.
#[derive(Debug, Clone, Default)]
pub struct CompleteApplicationObservations {
    observations: Arc<Mutex<Vec<CompleteApplicationObservation>>>,
    measurements: Arc<Mutex<Vec<CompleteApplicationMeasurement>>>,
}

impl CompleteApplicationObservations {
    pub fn record(&self, observation: CompleteApplicationObservation) {
        self.observations
            .lock()
            .expect("complete-application observations mutex poisoned")
            .push(observation);
    }

    pub fn record_measurement(&self, measurement: CompleteApplicationMeasurement) {
        self.measurements
            .lock()
            .expect("complete-application measurements mutex poisoned")
            .push(measurement);
    }

    pub fn snapshot(&self) -> Vec<CompleteApplicationObservation> {
        self.observations
            .lock()
            .expect("complete-application observations mutex poisoned")
            .clone()
    }

    pub fn measurements(&self) -> Vec<CompleteApplicationMeasurement> {
        self.measurements
            .lock()
            .expect("complete-application measurements mutex poisoned")
            .clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scripted_faults_are_fifo_and_consumed_once() {
        let script = CompleteApplicationFaultScript::default();
        script.fail_next(
            CompleteApplicationBoundary::Save,
            CompleteApplicationFault::Io,
        );
        script.fail_next(
            CompleteApplicationBoundary::Save,
            CompleteApplicationFault::Cancelled,
        );

        assert_eq!(
            script.take(CompleteApplicationBoundary::Save),
            Some(CompleteApplicationFault::Io)
        );
        assert_eq!(
            script.take(CompleteApplicationBoundary::Save),
            Some(CompleteApplicationFault::Cancelled)
        );
        assert_eq!(script.take(CompleteApplicationBoundary::Save), None);
    }

    #[test]
    fn scale_fixtures_are_deterministic_and_preserve_their_view_topology() {
        for (words, fixture) in [
            (
                NORMAL_DOCUMENT_WORDS,
                CompleteApplicationFixture::normal_with_topology(
                    ViewTopology::DistinctDocumentsTwoViews,
                ),
            ),
            (
                LARGE_DOCUMENT_WORDS,
                CompleteApplicationFixture::large_with_topology(
                    ViewTopology::DistinctDocumentsTwoViews,
                ),
            ),
        ] {
            let repeated = match words {
                NORMAL_DOCUMENT_WORDS => CompleteApplicationFixture::normal_with_topology(
                    ViewTopology::DistinctDocumentsTwoViews,
                ),
                LARGE_DOCUMENT_WORDS => CompleteApplicationFixture::large_with_topology(
                    ViewTopology::DistinctDocumentsTwoViews,
                ),
                _ => unreachable!("only declared fixture scales are tested"),
            };
            assert_eq!(fixture, repeated);
            assert_eq!(fixture.word_count(), words);
            assert_eq!(fixture.views(), ViewCount::Two);
            assert_eq!(fixture.topology(), ViewTopology::DistinctDocumentsTwoViews);
            let companion = fixture
                .companion_body()
                .expect("distinct views need a companion");
            assert_eq!(companion.split_whitespace().count(), words);
            assert_ne!(fixture.body(), companion);
        }
    }

    #[test]
    fn measurement_collection_does_not_invent_a_verdict() {
        let observations = CompleteApplicationObservations::default();
        observations.record_measurement(CompleteApplicationMeasurement {
            operation: "first editable viewport".to_owned(),
            elapsed: Duration::from_millis(25),
            resident_bytes: Some(1024),
            platform: "test-only".to_owned(),
            hardware_profile: None,
        });

        let recorded = observations.measurements();
        assert_eq!(recorded.len(), 1);
        assert_eq!(recorded[0].elapsed, Duration::from_millis(25));
        assert!(recorded[0].hardware_profile.is_none());
    }
}

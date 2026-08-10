//! Private offline implementation of the ParchMint `en-US` spellcheck service.
//!
//! The bundled dictionary engine, worker queue, and saved dictionary formats
//! stay behind ParchMint-owned values. No checked text leaves this process.

use std::{
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    error::Error,
    fmt,
    future::Future,
    pin::Pin,
    sync::{Arc, Condvar, Mutex, mpsc},
    task::{Context, Poll, Waker},
    thread,
};

use harper_core::{
    WordMetadata,
    spell::{Dictionary, FstDictionary, MutableDictionary, suggest_correct_spelling_str},
};
use parchmint_editor_api::{AsyncResult, DocumentPosition, EventStream};
use parchmint_spellcheck_api::{
    DictionaryRevision, DocumentId, LanguageId, ProjectId, SpellcheckGeneration, SpellcheckHandle,
    SpellcheckPriority, SpellcheckRequest, SpellcheckResult, SpellcheckResultStream,
    SpellcheckService, SpellingCategory, SpellingIssue, SpellingSuggestion, SuggestionRank,
    SuggestionRequest,
};

const DEFAULT_QUEUE_CAPACITY: usize = 64;
const DEFAULT_MAX_BLOCKS: usize = 64;
const DEFAULT_MAX_CHARS_PER_BLOCK: usize = 64 * 1024;
const DEFAULT_MAX_CHARS_PER_REQUEST: usize = 256 * 1024;
const MAX_WORD_CHARS: usize = 256;
const SUGGESTION_LIMIT: usize = 1;
const ENGINE_CANDIDATE_LIMIT: usize = 64;
const SUGGESTION_DISTANCE: u8 = 2;

/// A future returned by the concrete, fallible implementation.
pub type SpellcheckOperation<T> =
    Pin<Box<dyn Future<Output = Result<T, SpellcheckError>> + Send + 'static>>;

/// The bundled dictionary selected by this implementation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum BundledDictionarySource {
    /// Harper's curated US-English dictionary, compiled into `harper-core`.
    #[default]
    HarperCuratedEnUs,
}

/// Hard bounds applied before a check enters the background queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpellcheckWorkerLimits {
    pub queue_capacity: usize,
    pub max_blocks_per_request: usize,
    pub max_chars_per_block: usize,
    pub max_chars_per_request: usize,
}

impl Default for SpellcheckWorkerLimits {
    fn default() -> Self {
        Self {
            queue_capacity: DEFAULT_QUEUE_CAPACITY,
            max_blocks_per_request: DEFAULT_MAX_BLOCKS,
            max_chars_per_block: DEFAULT_MAX_CHARS_PER_BLOCK,
            max_chars_per_request: DEFAULT_MAX_CHARS_PER_REQUEST,
        }
    }
}

/// An error reported by a saved dictionary provider.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DictionaryLoadError {
    message: String,
}

impl DictionaryLoadError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for DictionaryLoadError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for DictionaryLoadError {}

/// Loads already-saved project and global dictionary revisions.
///
/// Persistence happens before these methods are called. A load failure must
/// therefore leave the saved source untouched so the same revision can be
/// retried later.
pub trait SavedDictionarySource: Send + Sync {
    fn project_words(
        &self,
        project: ProjectId,
        revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError>;

    fn global_words(
        &self,
        revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError>;
}

#[derive(Debug, Default)]
struct EmptyDictionarySource;

impl SavedDictionarySource for EmptyDictionarySource {
    fn project_words(
        &self,
        _project: ProjectId,
        _revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError> {
        Ok(Vec::new())
    }

    fn global_words(
        &self,
        _revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError> {
        Ok(Vec::new())
    }
}

/// Construction values for the private offline service.
#[derive(Clone)]
pub struct EnUsSpellcheckConfig {
    pub bundled_dictionary: BundledDictionarySource,
    pub worker_limits: SpellcheckWorkerLimits,
    pub saved_dictionaries: Arc<dyn SavedDictionarySource>,
}

impl Default for EnUsSpellcheckConfig {
    fn default() -> Self {
        Self {
            bundled_dictionary: BundledDictionarySource::default(),
            worker_limits: SpellcheckWorkerLimits::default(),
            saved_dictionaries: Arc::new(EmptyDictionarySource),
        }
    }
}

/// A failure to construct the private spelling runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellcheckStartupError {
    message: String,
}

impl fmt::Display for SpellcheckStartupError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl Error for SpellcheckStartupError {}

/// A recoverable failure from the concrete spellcheck implementation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpellcheckError {
    InvalidRequest(String),
    QueueFull,
    DictionaryReload {
        scope: &'static str,
        revision: DictionaryRevision,
        message: String,
    },
    WorkerStopped,
}

impl fmt::Display for SpellcheckError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRequest(message) => {
                write!(formatter, "invalid spellcheck request: {message}")
            }
            Self::QueueFull => formatter.write_str("spellcheck worker queue is full"),
            Self::DictionaryReload {
                scope,
                revision,
                message,
            } => write!(
                formatter,
                "failed to reload {scope} dictionary revision {}: {message}",
                revision.value()
            ),
            Self::WorkerStopped => formatter.write_str("spellcheck worker stopped"),
        }
    }
}

impl Error for SpellcheckError {}

/// The platform-neutral `en-US` spellcheck service.
#[derive(Clone)]
pub struct EnUsSpellcheckService {
    inner: Arc<ServiceInner>,
}

impl EnUsSpellcheckService {
    pub fn new(config: EnUsSpellcheckConfig) -> Result<Self, SpellcheckStartupError> {
        validate_limits(config.worker_limits)?;

        let runtime = std::panic::catch_unwind(PrivateSpellingRuntime::new).map_err(|_| {
            SpellcheckStartupError {
                message: "bundled en-US dictionary failed to initialize".to_owned(),
            }
        })?;
        let scheduler = Arc::new(SchedulerShared::new(config.worker_limits));
        let worker_scheduler = Arc::clone(&scheduler);
        let saved_dictionaries = Arc::clone(&config.saved_dictionaries);
        let worker = thread::Builder::new()
            .name("parchmint-spellcheck-en-us".to_owned())
            .spawn(move || worker_loop(worker_scheduler, runtime, saved_dictionaries))
            .map_err(|error| SpellcheckStartupError {
                message: format!("could not start spellcheck worker: {error}"),
            })?;

        Ok(Self {
            inner: Arc::new(ServiceInner {
                scheduler,
                worker: Mutex::new(Some(worker)),
            }),
        })
    }

    pub fn available_languages(&self) -> SpellcheckOperation<Vec<LanguageId>> {
        Box::pin(async { Ok(vec![LanguageId::EnUs]) })
    }

    pub fn check(&self, request: SpellcheckRequest) -> SpellcheckOperation<SpellcheckResultStream> {
        let (sender, receiver) = mpsc::channel();
        if let Err(error) = validate_request(&request, self.inner.scheduler.limits) {
            return Box::pin(async move { Err(error) });
        }

        let (completion_sender, completion) = completion();
        let work = Work::Check {
            request,
            delivery: CheckDelivery::Stream(sender),
            completion: completion_sender,
        };
        if self.inner.scheduler.enqueue(work).is_none() {
            return Box::pin(async { Err(SpellcheckError::WorkerStopped) });
        }

        Box::pin(async move {
            completion.await?;
            Ok(EventStream::from_receiver(receiver))
        })
    }

    pub fn suggest(
        &self,
        request: SuggestionRequest,
    ) -> SpellcheckOperation<Vec<SpellingSuggestion>> {
        if request.word.chars().count() > MAX_WORD_CHARS {
            return Box::pin(async {
                Err(SpellcheckError::InvalidRequest(format!(
                    "suggestion word exceeds the {MAX_WORD_CHARS} character limit"
                )))
            });
        }
        let (sender, completion) = completion();
        let work = Work::Suggest {
            request,
            completion: sender,
        };
        if self.inner.scheduler.enqueue(work).is_none() {
            return Box::pin(async { Err(SpellcheckError::WorkerStopped) });
        }
        Box::pin(completion)
    }

    pub fn cancel(&self, handle: SpellcheckHandle) {
        self.inner.scheduler.cancel(handle);
    }

    pub fn reload_project_dictionary(
        &self,
        project: ProjectId,
        revision: DictionaryRevision,
    ) -> SpellcheckOperation<()> {
        let (sender, completion) = completion();
        let work = Work::ReloadProject {
            project,
            revision,
            completion: sender,
        };
        if self.inner.scheduler.enqueue(work).is_none() {
            return Box::pin(async { Err(SpellcheckError::WorkerStopped) });
        }
        Box::pin(completion)
    }

    pub fn reload_global_dictionary(
        &self,
        revision: DictionaryRevision,
    ) -> SpellcheckOperation<()> {
        let (sender, completion) = completion();
        let work = Work::ReloadGlobal {
            revision,
            completion: sender,
        };
        if self.inner.scheduler.enqueue(work).is_none() {
            return Box::pin(async { Err(SpellcheckError::WorkerStopped) });
        }
        Box::pin(completion)
    }

    #[cfg(test)]
    fn enqueue_test_check(
        &self,
        request: SpellcheckRequest,
    ) -> (
        SpellcheckHandle,
        CompletionFuture<Result<Option<SpellcheckResult>, SpellcheckError>>,
    ) {
        let (sender, result_completion) = completion();
        let (work_completion, _work_finished) = completion();
        if let Err(error) = validate_request(&request, self.inner.scheduler.limits) {
            sender.complete(Err(error));
            return (SpellcheckHandle::default(), result_completion);
        }
        let work = Work::Check {
            request,
            delivery: CheckDelivery::Test(sender),
            completion: work_completion,
        };
        let handle = self.inner.scheduler.enqueue(work).unwrap_or_default();
        (handle, result_completion)
    }
}

impl Default for EnUsSpellcheckService {
    fn default() -> Self {
        Self::new(EnUsSpellcheckConfig::default())
            .expect("the bundled en-US dictionary and worker must initialize")
    }
}

impl SpellcheckService for EnUsSpellcheckService {
    fn available_languages(&self) -> AsyncResult<Vec<LanguageId>> {
        let operation = EnUsSpellcheckService::available_languages(self);
        Box::pin(async move { operation.await.unwrap_or_default() })
    }

    fn check(&self, request: SpellcheckRequest) -> AsyncResult<SpellcheckResultStream> {
        let operation = EnUsSpellcheckService::check(self, request);
        Box::pin(async move {
            operation.await.unwrap_or_else(|_| {
                let (_sender, receiver) = mpsc::channel();
                EventStream::from_receiver(receiver)
            })
        })
    }

    fn suggest(&self, request: SuggestionRequest) -> AsyncResult<Vec<SpellingSuggestion>> {
        let operation = EnUsSpellcheckService::suggest(self, request);
        Box::pin(async move { operation.await.unwrap_or_default() })
    }

    fn cancel(&self, handle: SpellcheckHandle) {
        EnUsSpellcheckService::cancel(self, handle);
    }

    fn reload_project_dictionary(
        &self,
        project: ProjectId,
        revision: DictionaryRevision,
    ) -> AsyncResult<()> {
        let operation = EnUsSpellcheckService::reload_project_dictionary(self, project, revision);
        Box::pin(async move {
            let _ = operation.await;
        })
    }

    fn reload_global_dictionary(&self, revision: DictionaryRevision) -> AsyncResult<()> {
        let operation = EnUsSpellcheckService::reload_global_dictionary(self, revision);
        Box::pin(async move {
            let _ = operation.await;
        })
    }
}

fn validate_limits(limits: SpellcheckWorkerLimits) -> Result<(), SpellcheckStartupError> {
    if limits.queue_capacity == 0
        || limits.max_blocks_per_request == 0
        || limits.max_chars_per_block == 0
        || limits.max_chars_per_request == 0
    {
        return Err(SpellcheckStartupError {
            message: "spellcheck worker limits must all be greater than zero".to_owned(),
        });
    }
    Ok(())
}

fn validate_request(
    request: &SpellcheckRequest,
    limits: SpellcheckWorkerLimits,
) -> Result<(), SpellcheckError> {
    if request.language != LanguageId::EnUs {
        return Err(SpellcheckError::InvalidRequest(
            "only en-US is available".to_owned(),
        ));
    }
    if request.blocks.len() > limits.max_blocks_per_request {
        return Err(SpellcheckError::InvalidRequest(format!(
            "{} blocks exceeds the {} block limit",
            request.blocks.len(),
            limits.max_blocks_per_request
        )));
    }

    let mut total_chars = 0_usize;
    for block in &request.blocks {
        let chars = block.text.chars().count();
        if chars > limits.max_chars_per_block {
            return Err(SpellcheckError::InvalidRequest(format!(
                "one block has {chars} characters, exceeding the {} character limit",
                limits.max_chars_per_block
            )));
        }
        total_chars = total_chars.saturating_add(chars);
    }
    if total_chars > limits.max_chars_per_request {
        return Err(SpellcheckError::InvalidRequest(format!(
            "{total_chars} characters exceeds the {} character request limit",
            limits.max_chars_per_request
        )));
    }
    Ok(())
}

struct ServiceInner {
    scheduler: Arc<SchedulerShared>,
    worker: Mutex<Option<thread::JoinHandle<()>>>,
}

impl Drop for ServiceInner {
    fn drop(&mut self) {
        self.scheduler.shutdown();
        if let Some(worker) = self.worker.lock().unwrap().take() {
            let _ = worker.join();
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
struct RequestStamp {
    generation: SpellcheckGeneration,
    document_revision: parchmint_spellcheck_api::EditorRevision,
    project_dictionary: DictionaryRevision,
    global_dictionary: DictionaryRevision,
}

impl From<&SpellcheckRequest> for RequestStamp {
    fn from(request: &SpellcheckRequest) -> Self {
        Self {
            generation: request.generation,
            document_revision: request.document_revision,
            project_dictionary: request.project_dictionary,
            global_dictionary: request.global_dictionary,
        }
    }
}

struct SchedulerShared {
    state: Mutex<SchedulerState>,
    changed: Condvar,
    limits: SpellcheckWorkerLimits,
}

struct SchedulerState {
    queue: Vec<QueuedWork>,
    next_handle: u64,
    next_sequence: u64,
    newest: HashMap<DocumentId, RequestStamp>,
    cancelled: HashSet<SpellcheckHandle>,
    paused: bool,
    shutdown: bool,
}

struct QueuedWork {
    handle: SpellcheckHandle,
    sequence: u64,
    work: Work,
}

enum Work {
    Check {
        request: SpellcheckRequest,
        delivery: CheckDelivery,
        completion: CompletionSender<Result<(), SpellcheckError>>,
    },
    Suggest {
        request: SuggestionRequest,
        completion: CompletionSender<Result<Vec<SpellingSuggestion>, SpellcheckError>>,
    },
    ReloadProject {
        project: ProjectId,
        revision: DictionaryRevision,
        completion: CompletionSender<Result<(), SpellcheckError>>,
    },
    ReloadGlobal {
        revision: DictionaryRevision,
        completion: CompletionSender<Result<(), SpellcheckError>>,
    },
}

enum CheckDelivery {
    Stream(mpsc::Sender<SpellcheckResult>),
    #[cfg(test)]
    Test(CompletionSender<Result<Option<SpellcheckResult>, SpellcheckError>>),
}

impl CheckDelivery {
    fn deliver(self, result: Option<SpellcheckResult>) {
        match self {
            Self::Stream(sender) => {
                if let Some(result) = result {
                    let _ = sender.send(result);
                }
            }
            #[cfg(test)]
            Self::Test(completion) => completion.complete(Ok(result)),
        }
    }

    fn fail(self, error: SpellcheckError) {
        #[cfg(not(test))]
        let _ = &error;
        match self {
            Self::Stream(_sender) => {}
            #[cfg(test)]
            Self::Test(completion) => completion.complete(Err(error)),
        }
    }
}

impl Work {
    fn priority(&self) -> Option<SpellcheckPriority> {
        match self {
            Self::Check { request, .. } => Some(request.priority),
            _ => None,
        }
    }

    fn scheduler_rank(&self) -> u8 {
        match self {
            Self::ReloadProject { .. } | Self::ReloadGlobal { .. } => 0,
            Self::Suggest { .. } => 1,
            Self::Check { request, .. } => match request.priority {
                SpellcheckPriority::Visible => 2,
                SpellcheckPriority::RecentlyChanged => 3,
                SpellcheckPriority::Background => 4,
            },
        }
    }

    fn complete_error(self, error: SpellcheckError) {
        match self {
            Self::Check {
                delivery,
                completion,
                ..
            } => {
                delivery.fail(error.clone());
                completion.complete(Err(error));
            }
            Self::Suggest { completion, .. } => {
                completion.complete(Err(error));
            }
            Self::ReloadProject { completion, .. } | Self::ReloadGlobal { completion, .. } => {
                completion.complete(Err(error));
            }
        }
    }

    fn drop_check(self) {
        if let Self::Check {
            delivery,
            completion,
            ..
        } = self
        {
            delivery.deliver(None);
            completion.complete(Ok(()));
        }
    }
}

impl SchedulerShared {
    fn new(limits: SpellcheckWorkerLimits) -> Self {
        Self {
            state: Mutex::new(SchedulerState {
                queue: Vec::new(),
                next_handle: 1,
                next_sequence: 1,
                newest: HashMap::new(),
                cancelled: HashSet::new(),
                paused: false,
                shutdown: false,
            }),
            changed: Condvar::new(),
            limits,
        }
    }

    fn enqueue(&self, work: Work) -> Option<SpellcheckHandle> {
        let mut state = self.state.lock().unwrap();
        if state.shutdown {
            drop(state);
            work.complete_error(SpellcheckError::WorkerStopped);
            return None;
        }

        let handle = SpellcheckHandle::new(state.next_handle);
        state.next_handle = state.next_handle.saturating_add(1);
        let sequence = state.next_sequence;
        state.next_sequence = state.next_sequence.saturating_add(1);

        if let Work::Check { request, .. } = &work {
            let stamp = RequestStamp::from(request);
            state
                .newest
                .entry(request.document_id)
                .and_modify(|current| {
                    if stamp > *current {
                        *current = stamp;
                    }
                })
                .or_insert(stamp);
        }

        state.queue.push(QueuedWork {
            handle,
            sequence,
            work,
        });

        let dropped = if state.queue.len() > self.limits.queue_capacity {
            let index = state
                .queue
                .iter()
                .enumerate()
                .filter(|(_, queued)| queued.work.priority().is_some())
                .max_by_key(|(_, queued)| {
                    (
                        queued.work.scheduler_rank(),
                        std::cmp::Reverse(queued.sequence),
                    )
                })
                .map(|(index, _)| index)
                .unwrap_or_else(|| {
                    state
                        .queue
                        .iter()
                        .position(|queued| queued.handle == handle)
                        .expect("the newly queued control operation is present")
                });
            Some(state.queue.remove(index).work)
        } else {
            None
        };

        drop(state);
        if let Some(dropped) = dropped {
            if dropped.priority().is_some() {
                dropped.drop_check();
            } else {
                dropped.complete_error(SpellcheckError::QueueFull);
            }
        }
        self.changed.notify_one();
        Some(handle)
    }

    fn cancel(&self, handle: SpellcheckHandle) {
        let mut state = self.state.lock().unwrap();
        state.cancelled.insert(handle);
        drop(state);
        self.changed.notify_one();
    }

    #[cfg(test)]
    fn pause(&self) {
        self.state.lock().unwrap().paused = true;
    }

    #[cfg(test)]
    fn resume(&self) {
        self.state.lock().unwrap().paused = false;
        self.changed.notify_all();
    }

    #[cfg(test)]
    fn queued_check_count(&self) -> usize {
        self.state
            .lock()
            .unwrap()
            .queue
            .iter()
            .filter(|queued| queued.work.priority().is_some())
            .count()
    }

    #[cfg(test)]
    fn queued_priorities(&self) -> Vec<SpellcheckPriority> {
        let mut priorities = self
            .state
            .lock()
            .unwrap()
            .queue
            .iter()
            .filter_map(|queued| queued.work.priority())
            .collect::<Vec<_>>();
        priorities.sort_unstable();
        priorities
    }

    fn is_current(&self, handle: SpellcheckHandle, request: &SpellcheckRequest) -> bool {
        let state = self.state.lock().unwrap();
        !state.cancelled.contains(&handle)
            && state.newest.get(&request.document_id) == Some(&RequestStamp::from(request))
    }

    fn take_next(&self) -> Option<QueuedWork> {
        let mut state = self.state.lock().unwrap();
        loop {
            if state.shutdown {
                let queued = std::mem::take(&mut state.queue);
                drop(state);
                for queued in queued {
                    queued.work.complete_error(SpellcheckError::WorkerStopped);
                }
                return None;
            }
            if !state.paused && !state.queue.is_empty() {
                let index = state
                    .queue
                    .iter()
                    .enumerate()
                    .min_by_key(|(_, queued)| {
                        (
                            queued.work.scheduler_rank(),
                            std::cmp::Reverse(queued.sequence),
                        )
                    })
                    .map(|(index, _)| index)
                    .expect("a non-empty queue has a next item");
                return Some(state.queue.remove(index));
            }
            state = self.changed.wait(state).unwrap();
        }
    }

    fn shutdown(&self) {
        self.state.lock().unwrap().shutdown = true;
        self.changed.notify_all();
    }
}

fn worker_loop(
    scheduler: Arc<SchedulerShared>,
    mut runtime: PrivateSpellingRuntime,
    saved_dictionaries: Arc<dyn SavedDictionarySource>,
) {
    while let Some(queued) = scheduler.take_next() {
        match queued.work {
            Work::Check {
                request,
                delivery,
                completion,
            } => {
                if !scheduler.is_current(queued.handle, &request) {
                    delivery.deliver(None);
                    completion.complete(Ok(()));
                    continue;
                }
                let result = runtime.check(&request);
                if scheduler.is_current(queued.handle, &request) {
                    delivery.deliver(Some(result));
                } else {
                    delivery.deliver(None);
                }
                completion.complete(Ok(()));
            }
            Work::Suggest {
                request,
                completion,
            } => completion.complete(Ok(runtime.suggest(&request))),
            Work::ReloadProject {
                project,
                revision,
                completion,
            } => {
                let result = saved_dictionaries
                    .project_words(project, revision)
                    .map_err(|error| SpellcheckError::DictionaryReload {
                        scope: "project",
                        revision,
                        message: error.to_string(),
                    })
                    .map(|words| runtime.reload_project(project, revision, words));
                completion.complete(result);
            }
            Work::ReloadGlobal {
                revision,
                completion,
            } => {
                let result = saved_dictionaries
                    .global_words(revision)
                    .map_err(|error| SpellcheckError::DictionaryReload {
                        scope: "global",
                        revision,
                        message: error.to_string(),
                    })
                    .map(|words| runtime.reload_global(revision, words));
                completion.complete(result);
            }
        }
    }
}

struct PrivateSpellingRuntime {
    bundled: Arc<FstDictionary>,
    project_words: BTreeMap<ProjectId, (DictionaryRevision, Arc<MutableDictionary>)>,
    global_words: Option<(DictionaryRevision, Arc<MutableDictionary>)>,
}

impl PrivateSpellingRuntime {
    fn new() -> Self {
        Self {
            bundled: FstDictionary::curated(),
            project_words: BTreeMap::new(),
            global_words: None,
        }
    }

    fn reload_project(
        &mut self,
        project: ProjectId,
        revision: DictionaryRevision,
        words: Vec<String>,
    ) {
        self.project_words
            .insert(project, (revision, Arc::new(dictionary_from_words(words))));
    }

    fn reload_global(&mut self, revision: DictionaryRevision, words: Vec<String>) {
        self.global_words = Some((revision, Arc::new(dictionary_from_words(words))));
    }

    fn contains(
        &self,
        word: &str,
        project: DictionaryRevision,
        global: DictionaryRevision,
    ) -> bool {
        self.bundled.contains_word_str(word)
            || self
                .project_words
                .values()
                .any(|(revision, words)| *revision == project && words.contains_word_str(word))
            || self.global_words.as_ref().is_some_and(|(revision, words)| {
                *revision == global && words.contains_word_str(word)
            })
    }

    fn suggestions(
        &self,
        word: &str,
        project: DictionaryRevision,
        global: DictionaryRevision,
    ) -> Vec<SpellingSuggestion> {
        if word.chars().count() > MAX_WORD_CHARS {
            return Vec::new();
        }
        let mut candidates = suggest_correct_spelling_str(
            word,
            ENGINE_CANDIDATE_LIMIT,
            SUGGESTION_DISTANCE,
            self.bundled.as_ref(),
        );

        for (_, dictionary) in self
            .project_words
            .values()
            .filter(|(revision, _)| *revision == project)
        {
            candidates.extend(suggest_correct_spelling_str(
                word,
                ENGINE_CANDIDATE_LIMIT,
                SUGGESTION_DISTANCE,
                dictionary.as_ref(),
            ));
        }
        if let Some((_, dictionary)) = self
            .global_words
            .as_ref()
            .filter(|(revision, _)| *revision == global)
        {
            candidates.extend(suggest_correct_spelling_str(
                word,
                ENGINE_CANDIDATE_LIMIT,
                SUGGESTION_DISTANCE,
                dictionary.as_ref(),
            ));
        }
        if let Some(transposition) = adjacent_transposition_correction(word, self.bundled.as_ref())
        {
            candidates.insert(0, transposition);
        }
        let mut seen = BTreeSet::new();
        candidates.retain(|candidate| seen.insert(candidate.clone()));

        candidates.truncate(SUGGESTION_LIMIT);

        candidates
            .into_iter()
            .enumerate()
            .map(|(rank, word)| SpellingSuggestion {
                word,
                rank: SuggestionRank::from(rank as u64),
            })
            .collect()
    }

    fn suggest(&self, request: &SuggestionRequest) -> Vec<SpellingSuggestion> {
        if self.contains(
            &request.word,
            request.project_dictionary,
            request.global_dictionary,
        ) {
            Vec::new()
        } else {
            self.suggestions(
                &request.word,
                request.project_dictionary,
                request.global_dictionary,
            )
        }
    }

    fn check(&self, request: &SpellcheckRequest) -> SpellcheckResult {
        let mut issues = Vec::new();
        for block in &request.blocks {
            let block_start = block.range.start().value();
            for token in word_tokens(&block.text) {
                if self.contains(
                    token.word,
                    request.project_dictionary,
                    request.global_dictionary,
                ) {
                    continue;
                }
                issues.push(SpellingIssue {
                    block_id: block.block_id,
                    range: parchmint_spellcheck_api::EditorSelection::new(
                        DocumentPosition::from(block_start.saturating_add(token.start as u64)),
                        DocumentPosition::from(block_start.saturating_add(token.end as u64)),
                    ),
                    word: token.word.to_owned(),
                    category: SpellingCategory::Misspelling,
                    suggestions: self.suggestions(
                        token.word,
                        request.project_dictionary,
                        request.global_dictionary,
                    ),
                });
            }
        }

        SpellcheckResult {
            document_id: request.document_id,
            document_revision: request.document_revision,
            project_dictionary: request.project_dictionary,
            global_dictionary: request.global_dictionary,
            generation: request.generation,
            issues,
        }
    }
}

fn adjacent_transposition_correction(word: &str, dictionary: &impl Dictionary) -> Option<String> {
    let mut characters = word.chars().collect::<Vec<_>>();
    for index in 0..characters.len().saturating_sub(1) {
        characters.swap(index, index + 1);
        let candidate = characters.iter().collect::<String>();
        if dictionary.contains_word_str(&candidate) {
            return Some(candidate);
        }
        characters.swap(index, index + 1);
    }
    None
}

fn dictionary_from_words(words: Vec<String>) -> MutableDictionary {
    let mut dictionary = MutableDictionary::new();
    let words = words
        .into_iter()
        .map(|word| word.trim().to_owned())
        .filter(|word| !word.is_empty())
        .collect::<BTreeSet<_>>();
    dictionary.extend_words(
        words
            .iter()
            .map(|word| (word.chars().collect::<Vec<_>>(), WordMetadata::default())),
    );
    dictionary
}

struct WordToken<'a> {
    word: &'a str,
    start: usize,
    end: usize,
}

fn word_tokens(text: &str) -> Vec<WordToken<'_>> {
    let chars = text.char_indices().collect::<Vec<_>>();
    let mut tokens = Vec::new();
    let mut start_char = None;

    for (char_index, (_, character)) in chars.iter().enumerate() {
        let is_inner_apostrophe = matches!(character, '\'' | '’')
            && start_char.is_some()
            && chars
                .get(char_index + 1)
                .is_some_and(|(_, next)| next.is_alphabetic());
        if character.is_alphabetic() || is_inner_apostrophe {
            start_char.get_or_insert(char_index);
        } else if let Some(start) = start_char.take() {
            push_token(text, &chars, start, char_index, &mut tokens);
        }
    }
    if let Some(start) = start_char {
        push_token(text, &chars, start, chars.len(), &mut tokens);
    }
    tokens
}

fn push_token<'a>(
    text: &'a str,
    chars: &[(usize, char)],
    start: usize,
    end: usize,
    tokens: &mut Vec<WordToken<'a>>,
) {
    let start_byte = chars[start].0;
    let end_byte = chars.get(end).map_or(text.len(), |(byte, _)| *byte);
    tokens.push(WordToken {
        word: &text[start_byte..end_byte],
        start,
        end,
    });
}

struct CompletionState<T> {
    value: Option<T>,
    waker: Option<Waker>,
}

struct CompletionShared<T> {
    state: Mutex<CompletionState<T>>,
}

struct CompletionSender<T> {
    shared: Arc<CompletionShared<T>>,
}

struct CompletionFuture<T> {
    shared: Arc<CompletionShared<T>>,
}

fn completion<T>() -> (CompletionSender<T>, CompletionFuture<T>) {
    let shared = Arc::new(CompletionShared {
        state: Mutex::new(CompletionState {
            value: None,
            waker: None,
        }),
    });
    (
        CompletionSender {
            shared: Arc::clone(&shared),
        },
        CompletionFuture { shared },
    )
}

impl<T> CompletionSender<T> {
    fn complete(self, value: T) {
        let waker = {
            let mut state = self.shared.state.lock().unwrap();
            state.value = Some(value);
            state.waker.take()
        };
        if let Some(waker) = waker {
            waker.wake();
        }
    }
}

impl<T> Future for CompletionFuture<T> {
    type Output = T;

    fn poll(self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<Self::Output> {
        let mut state = self.shared.state.lock().unwrap();
        if let Some(value) = state.value.take() {
            Poll::Ready(value)
        } else {
            state.waker = Some(context.waker().clone());
            Poll::Pending
        }
    }
}

#[cfg(test)]
mod spellcheck_en_us_contract_tests;

#[cfg(test)]
mod implementation_tests;

#[cfg(test)]
struct TestDictionarySource {
    state: Mutex<TestDictionaryState>,
}

#[cfg(test)]
#[derive(Default)]
struct TestDictionaryState {
    project: BTreeMap<(ProjectId, DictionaryRevision), BTreeSet<String>>,
    global: BTreeMap<DictionaryRevision, BTreeSet<String>>,
    fail_project: bool,
    fail_global: bool,
}

#[cfg(test)]
impl Default for TestDictionarySource {
    fn default() -> Self {
        Self {
            state: Mutex::new(TestDictionaryState::default()),
        }
    }
}

#[cfg(test)]
impl SavedDictionarySource for TestDictionarySource {
    fn project_words(
        &self,
        project: ProjectId,
        revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError> {
        let mut state = self.state.lock().unwrap();
        if std::mem::take(&mut state.fail_project) {
            return Err(DictionaryLoadError::new("injected project reload failure"));
        }
        Ok(state
            .project
            .get(&(project, revision))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect())
    }

    fn global_words(
        &self,
        revision: DictionaryRevision,
    ) -> Result<Vec<String>, DictionaryLoadError> {
        let mut state = self.state.lock().unwrap();
        if std::mem::take(&mut state.fail_global) {
            return Err(DictionaryLoadError::new("injected global reload failure"));
        }
        Ok(state
            .global
            .get(&revision)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect())
    }
}

/// Completion of one crate-local scheduled check.
#[cfg(test)]
type TestCheckCompletion = CompletionFuture<Result<Option<SpellcheckResult>, SpellcheckError>>;

/// Crate-local controls that exercise the real service in contract tests.
#[cfg(test)]
pub(crate) struct SpellcheckTestHarness {
    service: EnUsSpellcheckService,
    source: Arc<TestDictionarySource>,
    queued: Mutex<Vec<TestCheckCompletion>>,
}

#[cfg(test)]
impl SpellcheckTestHarness {
    pub(crate) fn new() -> Self {
        Self::with_queue_capacity(DEFAULT_QUEUE_CAPACITY)
    }

    pub(crate) fn with_queue_capacity(queue_capacity: usize) -> Self {
        let source = Arc::new(TestDictionarySource::default());
        let config = EnUsSpellcheckConfig {
            worker_limits: SpellcheckWorkerLimits {
                queue_capacity,
                ..SpellcheckWorkerLimits::default()
            },
            saved_dictionaries: source.clone(),
            ..EnUsSpellcheckConfig::default()
        };
        Self {
            service: EnUsSpellcheckService::new(config).expect("test spellcheck service"),
            source,
            queued: Mutex::new(Vec::new()),
        }
    }

    pub(crate) const fn service(&self) -> &EnUsSpellcheckService {
        &self.service
    }

    pub(crate) fn save_project_word(
        &self,
        project: ProjectId,
        revision: DictionaryRevision,
        word: &str,
    ) {
        self.source
            .state
            .lock()
            .unwrap()
            .project
            .entry((project, revision))
            .or_default()
            .insert(word.to_owned());
    }

    pub(crate) fn save_global_word(&self, revision: DictionaryRevision, word: &str) {
        self.source
            .state
            .lock()
            .unwrap()
            .global
            .entry(revision)
            .or_default()
            .insert(word.to_owned());
    }

    pub(crate) fn saved_project_words(
        &self,
        project: ProjectId,
        revision: DictionaryRevision,
    ) -> Vec<String> {
        self.source
            .state
            .lock()
            .unwrap()
            .project
            .get(&(project, revision))
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    pub(crate) fn fail_next_project_dictionary_reload(&self) {
        self.source.state.lock().unwrap().fail_project = true;
    }

    pub(crate) fn fail_next_global_dictionary_reload(&self) {
        self.source.state.lock().unwrap().fail_global = true;
    }

    pub(crate) fn pause_worker(&self) {
        self.service.inner.scheduler.pause();
    }

    pub(crate) fn resume_worker(&self) {
        self.service.inner.scheduler.resume();
    }

    pub(crate) fn enqueue(&self, request: SpellcheckRequest) -> SpellcheckHandle {
        let (handle, completion) = self.service.enqueue_test_check(request);
        self.queued.lock().unwrap().push(completion);
        handle
    }

    pub(crate) fn check(
        &self,
        request: SpellcheckRequest,
    ) -> Result<Vec<SpellingIssue>, SpellcheckError> {
        let (_handle, completion) = self.service.enqueue_test_check(request);
        block_on_test(completion).map(|result| result.map_or_else(Vec::new, |value| value.issues))
    }

    pub(crate) fn finish_queued_checks(&self) -> Result<Vec<SpellcheckResult>, SpellcheckError> {
        let completions = std::mem::take(&mut *self.queued.lock().unwrap());
        let mut results = Vec::new();
        for completion in completions {
            if let Some(result) = block_on_test(completion)? {
                results.push(result);
            }
        }
        Ok(results)
    }

    pub(crate) fn queued_request_count(&self) -> usize {
        self.service.inner.scheduler.queued_check_count()
    }

    pub(crate) fn queued_priorities(&self) -> Vec<SpellcheckPriority> {
        self.service.inner.scheduler.queued_priorities()
    }
}

#[cfg(test)]
fn block_on_test<F: Future>(future: F) -> F::Output {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    loop {
        match future.as_mut().poll(&mut context) {
            Poll::Ready(output) => return output,
            Poll::Pending => thread::yield_now(),
        }
    }
}

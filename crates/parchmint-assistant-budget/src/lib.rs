//! Durable reservation accounting without project text or credentials.

use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, OpenOptions},
    io::Write,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pricing {
    pub input_micros_per_million: u64,
    pub output_micros_per_million: u64,
    pub search_call_micros: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub search_calls: u64,
}

impl Pricing {
    pub fn estimate(self, usage: Usage) -> Result<u64, BudgetError> {
        let total = (u128::from(usage.input_tokens) * u128::from(self.input_micros_per_million))
            .checked_add(
                u128::from(usage.output_tokens) * u128::from(self.output_micros_per_million),
            )
            .ok_or(BudgetError::InvalidInput)?;
        let token_cost = total.div_ceil(1_000_000);
        u64::try_from(
            token_cost
                .checked_add(u128::from(usage.search_calls) * u128::from(self.search_call_micros))
                .ok_or(BudgetError::InvalidInput)?,
        )
        .map_err(|_| BudgetError::InvalidInput)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BudgetError {
    InvalidInput,
    LimitReached,
    Storage,
    UnknownReservation,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Reservation {
    period: String,
    reserved_micros: u64,
    actual_micros: Option<u64>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Journal {
    version: u32,
    reservations: BTreeMap<String, Reservation>,
}

/// One instance must be shared by all windows in a process.
pub struct BudgetJournal {
    path: PathBuf,
    state: Mutex<Journal>,
}

impl BudgetJournal {
    pub fn open(path: impl Into<PathBuf>) -> Result<Self, BudgetError> {
        let path = path.into();
        let state = match fs::read(&path) {
            Ok(bytes) => {
                let journal: Journal =
                    serde_json::from_slice(&bytes).map_err(|_| BudgetError::Storage)?;
                if journal.version != 1 {
                    return Err(BudgetError::Storage);
                }
                journal
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Journal {
                version: 1,
                ..Journal::default()
            },
            Err(_) => return Err(BudgetError::Storage),
        };
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    pub fn reserved_or_spent(&self, period: &str) -> Result<u64, BudgetError> {
        let state = self.state.lock().map_err(|_| BudgetError::Storage)?;
        total(&state, period)
    }

    /// Records the maximum estimated charge before dispatching a paid request.
    pub fn reserve(
        &self,
        period: &str,
        limit_micros: u64,
        maximum_micros: u64,
    ) -> Result<String, BudgetError> {
        if period.is_empty() || period.len() > 128 || limit_micros == 0 || maximum_micros == 0 {
            return Err(BudgetError::InvalidInput);
        }
        let mut state = self.state.lock().map_err(|_| BudgetError::Storage)?;
        if total(&state, period)?
            .checked_add(maximum_micros)
            .ok_or(BudgetError::InvalidInput)?
            > limit_micros
        {
            return Err(BudgetError::LimitReached);
        }
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_err(|_| BudgetError::Storage)?
            .as_nanos();
        let id = format!(
            "{}-{now}-{}",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        );
        let mut next = state.clone();
        if next.reservations.contains_key(&id) {
            return Err(BudgetError::Storage);
        }
        next.reservations.insert(
            id.clone(),
            Reservation {
                period: period.to_owned(),
                reserved_micros: maximum_micros,
                actual_micros: None,
            },
        );
        persist(&self.path, &next)?;
        *state = next;
        Ok(id)
    }

    /// A missing usage report must not call this: its reservation stays held.
    pub fn settle(&self, id: &str, actual_micros: u64) -> Result<(), BudgetError> {
        let mut state = self.state.lock().map_err(|_| BudgetError::Storage)?;
        let mut next = state.clone();
        let entry = next
            .reservations
            .get_mut(id)
            .ok_or(BudgetError::UnknownReservation)?;
        if entry.actual_micros.is_some() {
            return Err(BudgetError::UnknownReservation);
        }
        entry.actual_micros = Some(actual_micros);
        persist(&self.path, &next)?;
        *state = next;
        Ok(())
    }
}

fn total(journal: &Journal, period: &str) -> Result<u64, BudgetError> {
    journal
        .reservations
        .values()
        .filter(|entry| entry.period == period)
        .try_fold(0u64, |sum, entry| {
            sum.checked_add(entry.actual_micros.unwrap_or(entry.reserved_micros))
                .ok_or(BudgetError::InvalidInput)
        })
}

fn persist(path: &Path, journal: &Journal) -> Result<(), BudgetError> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent).map_err(|_| BudgetError::Storage)?;
    let sequence = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".assistant-usage-{}-{sequence}.tmp",
        std::process::id()
    ));
    let result = (|| {
        let bytes = serde_json::to_vec(journal).map_err(|_| BudgetError::Storage)?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|_| BudgetError::Storage)?;
        file.write_all(&bytes)
            .and_then(|_| file.sync_all())
            .map_err(|_| BudgetError::Storage)?;
        drop(file);
        fs::rename(&temporary, path).map_err(|_| BudgetError::Storage)
    })();
    if result.is_err() {
        let _ = fs::remove_file(temporary);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Barrier};

    #[test]
    fn concurrent_reservations_are_durable_and_unknown_outcomes_remain_held() {
        let path = std::env::temp_dir().join(format!(
            "parchmint-budget-{}-{}.json",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let journal = Arc::new(BudgetJournal::open(&path).unwrap());
        let barrier = Arc::new(Barrier::new(3));
        let workers = (0..2)
            .map(|_| {
                let journal = journal.clone();
                let barrier = barrier.clone();
                std::thread::spawn(move || {
                    barrier.wait();
                    journal.reserve("2026-10-15", 100, 60)
                })
            })
            .collect::<Vec<_>>();
        barrier.wait();
        let results = workers
            .into_iter()
            .map(|worker| worker.join().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        assert_eq!(
            results
                .iter()
                .filter(|result| **result == Err(BudgetError::LimitReached))
                .count(),
            1
        );
        let id = results.into_iter().find_map(Result::ok).unwrap();
        drop(journal);
        let reopened = BudgetJournal::open(&path).unwrap();
        assert_eq!(reopened.reserved_or_spent("2026-10-15"), Ok(60));
        reopened.settle(&id, 20).unwrap();
        assert_eq!(reopened.reserved_or_spent("2026-10-15"), Ok(20));
        assert_eq!(
            reopened.settle(&id, 1),
            Err(BudgetError::UnknownReservation)
        );
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn calculates_rounded_token_and_search_costs() {
        let pricing = Pricing {
            input_micros_per_million: 1_000_000,
            output_micros_per_million: 5_000_000,
            search_call_micros: 10_000,
        };
        assert_eq!(
            pricing.estimate(Usage {
                input_tokens: 12_000,
                output_tokens: 1_500,
                search_calls: 1
            }),
            Ok(29_500)
        );
    }

    #[test]
    fn definitely_unsent_reservation_can_be_settled_at_zero() {
        let path = std::env::temp_dir().join(format!(
            "parchmint-budget-unsent-{}-{}.json",
            std::process::id(),
            NEXT_ID.fetch_add(1, Ordering::Relaxed)
        ));
        let journal = BudgetJournal::open(&path).unwrap();
        let id = journal.reserve("period", 100, 60).unwrap();
        journal.settle(&id, 0).unwrap();
        assert_eq!(journal.reserved_or_spent("period"), Ok(0));
        drop(journal);
        assert_eq!(
            BudgetJournal::open(&path)
                .unwrap()
                .reserved_or_spent("period"),
            Ok(0)
        );
        fs::remove_file(path).unwrap();
    }
}

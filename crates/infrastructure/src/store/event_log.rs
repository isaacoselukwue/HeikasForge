use std::fs;
use std::io::{self, BufRead, BufReader};
use std::path::PathBuf;
use std::sync::Arc;

use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::store::ChainVerification;
use heikas_domain::clock::Timestamp;
use heikas_domain::event::{DurableEvent, EventPayload, GENESIS_HASH};
use heikas_domain::identity::{EventId, RunId};
use tokio::sync::Mutex;
use tracing::warn;

use crate::atomic::{append_line_synchronised, storage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChainTail {
    pub sequence: u64,
    pub hash: String,
}

impl Default for ChainTail {
    fn default() -> Self {
        Self {
            sequence: 0,
            hash: GENESIS_HASH.to_string(),
        }
    }
}

pub struct EventLogFile {
    path: PathBuf,
    quarantine_path: PathBuf,
    tail: Arc<Mutex<Option<ChainTail>>>,
}

impl EventLogFile {
    pub fn new(path: PathBuf, quarantine_path: PathBuf) -> Self {
        Self {
            path,
            quarantine_path,
            tail: Arc::new(Mutex::new(None)),
        }
    }

    pub async fn append(
        &self,
        run_id: RunId,
        event_id: EventId,
        recorded_at: Timestamp,
        payload: EventPayload,
    ) -> ApplicationResult<DurableEvent> {
        let mut guard = self.tail.lock().await;
        let tail = match guard.clone() {
            Some(tail) => tail,
            None => {
                let verification = self.verify_internal()?;
                ChainTail {
                    sequence: verification.last_sequence,
                    hash: verification.last_hash,
                }
            }
        };
        let event = DurableEvent::seal(
            tail.sequence + 1,
            event_id,
            run_id,
            recorded_at,
            &tail.hash,
            payload,
        )?;
        let line = serde_json::to_vec(&event)
            .map_err(|error| ApplicationError::Serialisation(error.to_string()))?;
        append_line_synchronised(&self.path, &line)?;
        *guard = Some(ChainTail {
            sequence: event.sequence,
            hash: event.chain_hash(),
        });
        Ok(event)
    }

    pub fn read_after(&self, sequence: u64) -> ApplicationResult<Vec<DurableEvent>> {
        Ok(self
            .read_all()?
            .into_iter()
            .filter(|event| event.sequence > sequence)
            .collect())
    }

    pub fn read_range(
        &self,
        from_sequence: u64,
        limit: usize,
    ) -> ApplicationResult<Vec<DurableEvent>> {
        Ok(self
            .read_all()?
            .into_iter()
            .filter(|event| event.sequence > from_sequence)
            .take(limit)
            .collect())
    }

    pub fn read_all(&self) -> ApplicationResult<Vec<DurableEvent>> {
        let (events, _) = self.read_complete_records()?;
        let mut previous_hash = GENESIS_HASH.to_string();
        for (expected_sequence, event) in (1..).zip(events.iter()) {
            event.verify(expected_sequence, &previous_hash)?;
            previous_hash = event.chain_hash();
        }
        Ok(events)
    }

    pub async fn verify(&self) -> ApplicationResult<ChainVerification> {
        let verification = self.verify_internal()?;
        let mut guard = self.tail.lock().await;
        *guard = Some(ChainTail {
            sequence: verification.last_sequence,
            hash: verification.last_hash.clone(),
        });
        Ok(verification)
    }

    fn verify_internal(&self) -> ApplicationResult<ChainVerification> {
        let (events, partial) = self.read_complete_records()?;
        let mut previous_hash = GENESIS_HASH.to_string();
        for (expected_sequence, event) in (1..).zip(events.iter()) {
            event.verify(expected_sequence, &previous_hash)?;
            previous_hash = event.chain_hash();
        }
        Ok(ChainVerification {
            events_verified: events.len() as u64,
            last_sequence: events.last().map(|event| event.sequence).unwrap_or(0),
            last_hash: previous_hash,
            quarantined_partial_record: partial,
        })
    }

    fn read_complete_records(&self) -> ApplicationResult<(Vec<DurableEvent>, bool)> {
        let file = match fs::File::open(&self.path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                return Ok((Vec::new(), false))
            }
            Err(error) => return Err(storage(&self.path, "open", error)),
        };
        let reader = BufReader::new(file);
        let mut events = Vec::new();
        let mut partial = false;
        let mut pending_partial: Option<String> = None;

        for entry in reader.split(b'\n') {
            let bytes = entry.map_err(|error| storage(&self.path, "read", error))?;
            if bytes.is_empty() {
                continue;
            }
            if let Some(previous) = pending_partial.take() {
                self.quarantine(&previous)?;
                partial = true;
            }
            let text = String::from_utf8_lossy(&bytes).into_owned();
            match serde_json::from_str::<DurableEvent>(&text) {
                Ok(event) => events.push(event),
                Err(error) => {
                    warn!(error = %error, "an event record could not be decoded");
                    pending_partial = Some(text);
                }
            }
        }

        if let Some(remaining) = pending_partial {
            self.quarantine(&remaining)?;
            partial = true;
            self.truncate_partial_tail(&remaining)?;
        }

        Ok((events, partial))
    }

    fn quarantine(&self, record: &str) -> ApplicationResult<()> {
        append_line_synchronised(&self.quarantine_path, record.as_bytes())
    }

    fn truncate_partial_tail(&self, record: &str) -> ApplicationResult<()> {
        let contents = fs::read(&self.path).map_err(|error| storage(&self.path, "read", error))?;
        let record_bytes = record.as_bytes();
        if contents.len() < record_bytes.len() {
            return Ok(());
        }
        let boundary = contents.len() - record_bytes.len();
        if &contents[boundary..] != record_bytes {
            return Ok(());
        }
        let truncated = &contents[..boundary];
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&self.path)
            .map_err(|error| storage(&self.path, "open for truncation", error))?;
        file.set_len(truncated.len() as u64)
            .map_err(|error| storage(&self.path, "truncate", error))?;
        file.sync_all()
            .map_err(|error| storage(&self.path, "synchronise", error))?;
        Ok(())
    }
}

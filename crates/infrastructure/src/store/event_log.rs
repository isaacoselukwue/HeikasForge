use std::fs;
use std::io::{self, BufRead, BufReader, Seek, SeekFrom};
use std::path::PathBuf;
use std::sync::{Arc, Mutex as SyncMutex};

use heikas_application::error::{ApplicationError, ApplicationResult};
use heikas_application::ports::store::ChainVerification;
use heikas_domain::clock::Timestamp;
use heikas_domain::event::{DurableEvent, EventPayload, GENESIS_HASH};
use heikas_domain::identity::{EventId, RunId};
use tokio::sync::Mutex;
use tracing::warn;

use crate::atomic::{append_line_synchronised, storage};

pub const MAXIMUM_RECORD_BYTES: usize = 4_194_304;

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

#[derive(Debug, Default)]
struct VerifiedPrefix {
    events: Vec<DurableEvent>,
    verified_bytes: u64,
    last_hash: String,
}

impl VerifiedPrefix {
    fn empty() -> Self {
        Self {
            events: Vec::new(),
            verified_bytes: 0,
            last_hash: GENESIS_HASH.to_string(),
        }
    }
}

pub struct EventLogFile {
    path: PathBuf,
    quarantine_path: PathBuf,
    tail: Arc<Mutex<Option<ChainTail>>>,
    prefix: SyncMutex<VerifiedPrefix>,
}

impl EventLogFile {
    pub fn new(path: PathBuf, quarantine_path: PathBuf) -> Self {
        Self {
            path,
            quarantine_path,
            tail: Arc::new(Mutex::new(None)),
            prefix: SyncMutex::new(VerifiedPrefix::empty()),
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
        if line.len() > MAXIMUM_RECORD_BYTES {
            return Err(ApplicationError::Storage(format!(
                "an event record of {} bytes exceeds the {MAXIMUM_RECORD_BYTES} byte record limit",
                line.len()
            )));
        }
        append_line_synchronised(&self.path, &line)?;
        *guard = Some(ChainTail {
            sequence: event.sequence,
            hash: event.chain_hash(),
        });
        Ok(event)
    }

    pub fn read_after(&self, sequence: u64) -> ApplicationResult<Vec<DurableEvent>> {
        let (events, _) = self.load()?;
        Ok(events
            .into_iter()
            .filter(|event| event.sequence > sequence)
            .collect())
    }

    pub fn read_range(
        &self,
        from_sequence: u64,
        limit: usize,
    ) -> ApplicationResult<Vec<DurableEvent>> {
        let (events, _) = self.load()?;
        Ok(events
            .into_iter()
            .filter(|event| event.sequence > from_sequence)
            .take(limit)
            .collect())
    }

    pub fn read_all(&self) -> ApplicationResult<Vec<DurableEvent>> {
        Ok(self.load()?.0)
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
        let (events, partial) = self.load()?;
        let last_hash = events
            .last()
            .map(|event| event.chain_hash())
            .unwrap_or_else(|| GENESIS_HASH.to_string());
        Ok(ChainVerification {
            events_verified: events.len() as u64,
            last_sequence: events.last().map(|event| event.sequence).unwrap_or(0),
            last_hash,
            quarantined_partial_record: partial,
        })
    }

    fn load(&self) -> ApplicationResult<(Vec<DurableEvent>, bool)> {
        let mut prefix = self.prefix.lock().map_err(|_| {
            ApplicationError::Internal("the event log cache is poisoned".to_string())
        })?;
        let metadata = match fs::metadata(&self.path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                *prefix = VerifiedPrefix::empty();
                return Ok((Vec::new(), false));
            }
            Err(error) => return Err(storage(&self.path, "inspect", error)),
        };
        let length = metadata.len();
        if length < prefix.verified_bytes {
            *prefix = VerifiedPrefix::empty();
        }
        if length == prefix.verified_bytes {
            return Ok((prefix.events.clone(), false));
        }

        let mut file =
            fs::File::open(&self.path).map_err(|error| storage(&self.path, "open", error))?;
        file.seek(SeekFrom::Start(prefix.verified_bytes))
            .map_err(|error| storage(&self.path, "seek", error))?;
        let mut reader = BufReader::new(file);

        let mut appended: Vec<DurableEvent> = Vec::new();
        let mut consumed = prefix.verified_bytes;
        let mut previous_hash = prefix.last_hash.clone();
        let mut next_sequence = prefix.events.len() as u64 + 1;
        let mut partial = false;
        let mut pending: Option<Vec<u8>> = None;

        loop {
            let mut record = Vec::new();
            let read = read_bounded_line(&mut reader, &mut record)
                .map_err(|error| storage(&self.path, "read", error))?;
            if read == 0 {
                break;
            }
            let terminated = record.last() == Some(&b'\n');
            while matches!(record.last(), Some(b'\n') | Some(b'\r')) {
                record.pop();
            }
            if record.is_empty() {
                consumed += read as u64;
                continue;
            }
            if let Some(previous) = pending.take() {
                self.quarantine(&previous)?;
                partial = true;
            }
            if !terminated {
                pending = Some(record);
                break;
            }
            match serde_json::from_slice::<DurableEvent>(&record) {
                Ok(event) => {
                    event.verify(next_sequence, &previous_hash)?;
                    previous_hash = event.chain_hash();
                    next_sequence += 1;
                    appended.push(event);
                    consumed += read as u64;
                }
                Err(error) => {
                    warn!(error = %error, "an event record could not be decoded");
                    pending = Some(record);
                }
            }
        }

        if let Some(remaining) = pending {
            self.quarantine(&remaining)?;
            partial = true;
            self.truncate_partial_tail(&remaining)?;
        }

        prefix.events.extend(appended);
        prefix.verified_bytes = consumed;
        prefix.last_hash = previous_hash;
        Ok((prefix.events.clone(), partial))
    }

    fn quarantine(&self, record: &[u8]) -> ApplicationResult<()> {
        append_line_synchronised(&self.quarantine_path, record)
    }

    fn truncate_partial_tail(&self, record: &[u8]) -> ApplicationResult<()> {
        let contents = fs::read(&self.path).map_err(|error| storage(&self.path, "read", error))?;
        if contents.len() < record.len() {
            return Ok(());
        }
        let boundary = contents.len() - record.len();
        if &contents[boundary..] != record {
            return Ok(());
        }
        let file = fs::OpenOptions::new()
            .write(true)
            .open(&self.path)
            .map_err(|error| storage(&self.path, "open for truncation", error))?;
        file.set_len(boundary as u64)
            .map_err(|error| storage(&self.path, "truncate", error))?;
        file.sync_all()
            .map_err(|error| storage(&self.path, "synchronise", error))?;
        Ok(())
    }
}

fn read_bounded_line<R: BufRead>(reader: &mut R, target: &mut Vec<u8>) -> io::Result<usize> {
    let mut total = 0usize;
    loop {
        let available = reader.fill_buf()?;
        if available.is_empty() {
            return Ok(total);
        }
        match available.iter().position(|byte| *byte == b'\n') {
            Some(index) => {
                target.extend_from_slice(&available[..=index]);
                reader.consume(index + 1);
                return Ok(total + index + 1);
            }
            None => {
                let length = available.len();
                target.extend_from_slice(available);
                reader.consume(length);
                total += length;
                if total > MAXIMUM_RECORD_BYTES {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("an event record exceeds the {MAXIMUM_RECORD_BYTES} byte limit"),
                    ));
                }
            }
        }
    }
}

use std::collections::HashMap;
use std::sync::Arc;

use heikas_application::error::ApplicationResult;
use heikas_domain::identity::RunId;
use heikas_infrastructure::Runtime;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tracing::{info, warn};

use crate::session::SessionManager;

#[derive(Clone)]
pub struct ApiState {
    pub runtime: Runtime,
    pub sessions: Arc<SessionManager>,
    pub dispatches: Arc<Mutex<HashMap<RunId, JoinHandle<()>>>>,
    pub origin: Arc<Mutex<Option<String>>>,
    pub demonstration_mode: bool,
}

impl ApiState {
    pub fn new(runtime: Runtime, demonstration_mode: bool) -> Self {
        Self {
            runtime,
            sessions: Arc::new(SessionManager::new()),
            dispatches: Arc::new(Mutex::new(HashMap::new())),
            origin: Arc::new(Mutex::new(None)),
            demonstration_mode,
        }
    }

    pub async fn set_origin(&self, origin: String) {
        let mut guard = self.origin.lock().await;
        *guard = Some(origin);
    }

    pub async fn expected_origin(&self) -> Option<String> {
        self.origin.lock().await.clone()
    }

    pub async fn spawn_dispatch(&self, run_id: RunId) -> ApplicationResult<()> {
        let mut guard = self.dispatches.lock().await;
        guard.retain(|_, handle| !handle.is_finished());
        if guard.contains_key(&run_id) {
            return Ok(());
        }
        let service = Arc::clone(&self.runtime.service);
        let handle = tokio::spawn(async move {
            match service.dispatch(run_id).await {
                Ok(outcome) => info!(run_id = %run_id, outcome = ?outcome, "dispatch finished"),
                Err(error) => warn!(run_id = %run_id, error = %error, "dispatch failed"),
            }
        });
        guard.insert(run_id, handle);
        Ok(())
    }

    pub async fn active_dispatches(&self) -> usize {
        let mut guard = self.dispatches.lock().await;
        guard.retain(|_, handle| !handle.is_finished());
        guard.len()
    }
}

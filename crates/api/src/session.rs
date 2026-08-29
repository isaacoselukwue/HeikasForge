use std::collections::HashMap;
use std::time::{Duration, Instant};

use rand::Rng;
use tokio::sync::Mutex;

pub const SESSION_COOKIE: &str = "heikas_session";
pub const CSRF_COOKIE: &str = "heikas_csrf";
pub const CSRF_HEADER: &str = "x-heikas-csrf";
pub const BOOTSTRAP_HEADER: &str = "x-heikas-bootstrap";

const SESSION_LIFETIME: Duration = Duration::from_secs(60 * 60 * 12);
const MUTATION_WINDOW: Duration = Duration::from_secs(10);
const MUTATION_ALLOWANCE: u32 = 40;

#[derive(Debug, Clone)]
pub struct Session {
    pub id: String,
    pub csrf_token: String,
    pub created_at: Instant,
    pub mutation_window_started: Instant,
    pub mutations_in_window: u32,
}

pub struct SessionManager {
    bootstrap_token: String,
    sessions: Mutex<HashMap<String, Session>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionRejection {
    Missing,
    Expired,
    CsrfMismatch,
    RateLimited,
}

impl SessionRejection {
    pub fn message(&self) -> &'static str {
        match self {
            SessionRejection::Missing => "no valid session cookie was supplied",
            SessionRejection::Expired => "the session has expired, reload the interface",
            SessionRejection::CsrfMismatch => "the cross-site request forgery token did not match",
            SessionRejection::RateLimited => {
                "too many state-changing requests were made, slow down"
            }
        }
    }
}

impl SessionManager {
    pub fn new() -> Self {
        Self {
            bootstrap_token: random_token(),
            sessions: Mutex::new(HashMap::new()),
        }
    }

    pub fn bootstrap_token(&self) -> &str {
        &self.bootstrap_token
    }

    pub async fn exchange(&self, presented: &str) -> Option<Session> {
        if !constant_time_equals(presented.as_bytes(), self.bootstrap_token.as_bytes()) {
            return None;
        }
        let session = Session {
            id: random_token(),
            csrf_token: random_token(),
            created_at: Instant::now(),
            mutation_window_started: Instant::now(),
            mutations_in_window: 0,
        };
        let mut guard = self.sessions.lock().await;
        guard.retain(|_, existing| existing.created_at.elapsed() < SESSION_LIFETIME);
        guard.insert(session.id.clone(), session.clone());
        Some(session)
    }

    pub async fn validate(
        &self,
        session_id: Option<&str>,
        csrf: Option<&str>,
        mutating: bool,
    ) -> Result<Session, SessionRejection> {
        let Some(session_id) = session_id else {
            return Err(SessionRejection::Missing);
        };
        let mut guard = self.sessions.lock().await;
        let Some(session) = guard.get_mut(session_id) else {
            return Err(SessionRejection::Missing);
        };
        if session.created_at.elapsed() >= SESSION_LIFETIME {
            guard.remove(session_id);
            return Err(SessionRejection::Expired);
        }
        if mutating {
            let Some(csrf) = csrf else {
                return Err(SessionRejection::CsrfMismatch);
            };
            if !constant_time_equals(csrf.as_bytes(), session.csrf_token.as_bytes()) {
                return Err(SessionRejection::CsrfMismatch);
            }
            if session.mutation_window_started.elapsed() > MUTATION_WINDOW {
                session.mutation_window_started = Instant::now();
                session.mutations_in_window = 0;
            }
            session.mutations_in_window += 1;
            if session.mutations_in_window > MUTATION_ALLOWANCE {
                return Err(SessionRejection::RateLimited);
            }
        }
        Ok(session.clone())
    }
}

impl Default for SessionManager {
    fn default() -> Self {
        Self::new()
    }
}

fn random_token() -> String {
    let mut generator = rand::rng();
    let bytes: [u8; 32] = generator.random();
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn constant_time_equals(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    let mut difference = 0u8;
    for (a, b) in left.iter().zip(right.iter()) {
        difference |= a ^ b;
    }
    difference == 0
}

use heikas_domain::clock::Timestamp;
use heikas_domain::identity::{ApprovalId, EventId, RunId};

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
}

pub trait IdentifierFactory: Send + Sync {
    fn new_run_id(&self) -> RunId;
    fn new_event_id(&self) -> EventId;
    fn new_approval_id(&self) -> ApprovalId;
    fn jitter_fraction(&self) -> f64;
}

pub trait LocalIdentity: Send + Sync {
    fn user_name(&self) -> String;
}

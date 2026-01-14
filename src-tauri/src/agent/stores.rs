use std::collections::HashMap;
use std::sync::{mpsc, Arc, Mutex};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub enum StepApprovalDecision {
    Approved,
    Skipped,
    Modified { feedback: Option<String> },
    Denied { feedback: Option<String> },
}

#[derive(Clone)]
pub struct StepApprovalStore {
    pending: Arc<Mutex<HashMap<String, mpsc::Sender<StepApprovalDecision>>>>,
}

impl StepApprovalStore {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create_request(&self) -> (String, mpsc::Receiver<StepApprovalDecision>) {
        let (tx, rx) = mpsc::channel();
        let approval_id = Uuid::new_v4().to_string();
        let mut pending = self.pending.lock().unwrap();
        pending.insert(approval_id.clone(), tx);
        (approval_id, rx)
    }

    pub fn resolve(
        &self,
        approval_id: &str,
        decision: StepApprovalDecision,
    ) -> Result<(), String> {
        let sender = {
            let mut pending = self.pending.lock().unwrap();
            pending.remove(approval_id)
        };

        let sender = sender.ok_or_else(|| format!("Unknown approval id: {approval_id}"))?;
        sender
            .send(decision)
            .map_err(|_| "Failed to deliver approval decision".to_string())
    }
}

#[derive(Clone)]
pub struct HumanInputStore {
    pending: Arc<Mutex<HashMap<String, mpsc::Sender<String>>>>,
}

impl HumanInputStore {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn create_request(&self) -> (String, mpsc::Receiver<String>) {
        let (tx, rx) = mpsc::channel();
        let request_id = Uuid::new_v4().to_string();
        let mut pending = self.pending.lock().unwrap();
        pending.insert(request_id.clone(), tx);
        (request_id, rx)
    }

    pub fn resolve(&self, request_id: &str, input: String) -> Result<(), String> {
        let sender = {
            let mut pending = self.pending.lock().unwrap();
            pending.remove(request_id)
        };

        let sender = sender.ok_or_else(|| format!("Unknown input request id: {request_id}"))?;
        sender
            .send(input)
            .map_err(|_| "Failed to deliver human input".to_string())
    }
}

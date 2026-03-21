use serde::{Deserialize, Serialize};
use std::fmt;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Pending,
    InProgress,
    Done,
}

impl fmt::Display for TaskStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TaskStatus::Pending => write!(f, "pending"),
            TaskStatus::InProgress => write!(f, "in_progress"),
            TaskStatus::Done => write!(f, "done"),
        }
    }
}

impl TaskStatus {
    pub fn from_str_checked(s: &str) -> Result<Self, String> {
        match s {
            "pending" => Ok(TaskStatus::Pending),
            "in_progress" => Ok(TaskStatus::InProgress),
            "done" => Ok(TaskStatus::Done),
            other => Err(format!("Invalid status: '{other}'. Valid values: pending, in_progress, done")),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    pub id: u64,
    pub title: String,
    pub description: String,
    pub priority: u8,
    pub status: TaskStatus,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
pub struct CreateTaskRequest {
    pub title: String,
    pub description: String,
    pub priority: u8,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

pub fn validate_priority(p: u8) -> Result<u8, String> {
    if (1..=5).contains(&p) {
        Ok(p)
    } else {
        Err(format!("Priority must be between 1 and 5, got {p}"))
    }
}

pub fn validate_transition(current: &TaskStatus, next: &TaskStatus) -> Result<(), String> {
    let allowed = matches!(
        (current, next),
        (TaskStatus::Pending, TaskStatus::InProgress)
            | (TaskStatus::InProgress, TaskStatus::Done)
            | (TaskStatus::InProgress, TaskStatus::Pending)
    );

    if allowed {
        Ok(())
    } else {
        Err(format!(
            "Invalid transition from {current} to {next}"
        ))
    }
}

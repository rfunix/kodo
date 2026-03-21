use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::models::{
    validate_priority, validate_transition, CreateTaskRequest, Task, TaskStatus,
};

#[derive(Debug, Clone, serde::Serialize)]
pub struct TaskStats {
    pub total: usize,
    pub pending: usize,
    pub in_progress: usize,
    pub done: usize,
    pub high_priority: usize,
}

#[derive(Clone)]
pub struct TaskService {
    tasks: Arc<RwLock<HashMap<u64, Task>>>,
    next_id: Arc<AtomicU64>,
}

impl TaskService {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(HashMap::new())),
            next_id: Arc::new(AtomicU64::new(1)),
        }
    }

    pub async fn create_task(&self, req: CreateTaskRequest) -> Result<Task, String> {
        validate_priority(req.priority)?;

        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let task = Task {
            id,
            title: req.title,
            description: req.description,
            priority: req.priority,
            status: TaskStatus::Pending,
            created_at: now_iso8601(),
        };

        self.tasks.write().await.insert(id, task.clone());
        Ok(task)
    }

    pub async fn get_task(&self, id: u64) -> Result<Task, String> {
        self.tasks
            .read()
            .await
            .get(&id)
            .cloned()
            .ok_or_else(|| format!("Task {id} not found"))
    }

    pub async fn list_tasks(&self) -> Vec<Task> {
        let map = self.tasks.read().await;
        let mut tasks: Vec<Task> = map.values().cloned().collect();
        tasks.sort_by_key(|t| t.id);
        tasks
    }

    pub async fn update_status(&self, id: u64, new_status: &str) -> Result<Task, String> {
        let next = TaskStatus::from_str_checked(new_status)?;
        let mut map = self.tasks.write().await;
        let task = map.get_mut(&id).ok_or_else(|| format!("Task {id} not found"))?;
        validate_transition(&task.status, &next)?;
        task.status = next;
        Ok(task.clone())
    }

    pub async fn delete_task(&self, id: u64) -> Result<Task, String> {
        self.tasks
            .write()
            .await
            .remove(&id)
            .ok_or_else(|| format!("Task {id} not found"))
    }

    pub async fn get_stats(&self) -> TaskStats {
        let map = self.tasks.read().await;
        let mut stats = TaskStats {
            total: map.len(),
            pending: 0,
            in_progress: 0,
            done: 0,
            high_priority: 0,
        };
        for task in map.values() {
            match task.status {
                TaskStatus::Pending => stats.pending += 1,
                TaskStatus::InProgress => stats.in_progress += 1,
                TaskStatus::Done => stats.done += 1,
            }
            if task.priority >= 4 {
                stats.high_priority += 1;
            }
        }
        stats
    }

    pub async fn load_tasks(&self, tasks: HashMap<u64, Task>) {
        let max_id = tasks.keys().copied().max().unwrap_or(0);
        self.next_id.store(max_id + 1, Ordering::Relaxed);
        *self.tasks.write().await = tasks;
    }
}

fn now_iso8601() -> String {
    let duration = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Simple UTC timestamp without external crate
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Days since 1970-01-01
    let mut y = 1970i64;
    let mut remaining_days = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }
    let month_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 0usize;
    for (i, &md) in month_days.iter().enumerate() {
        if remaining_days < md as i64 {
            m = i;
            break;
        }
        remaining_days -= md as i64;
    }
    let d = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y,
        m + 1,
        d,
        hours,
        minutes,
        seconds
    )
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

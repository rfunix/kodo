#[cfg(test)]
mod tests {
    use crate::models::{validate_priority, validate_transition, TaskStatus};
    use crate::service::TaskService;
    use crate::models::CreateTaskRequest;
    use crate::persistence;
    use std::collections::HashMap;

    // --- Model validation tests ---

    #[test]
    fn test_validate_priority_valid() {
        for p in 1..=5 {
            assert!(validate_priority(p).is_ok());
        }
    }

    #[test]
    fn test_validate_priority_invalid() {
        assert!(validate_priority(0).is_err());
        assert!(validate_priority(6).is_err());
        assert!(validate_priority(255).is_err());
    }

    #[test]
    fn test_valid_transitions() {
        assert!(validate_transition(&TaskStatus::Pending, &TaskStatus::InProgress).is_ok());
        assert!(validate_transition(&TaskStatus::InProgress, &TaskStatus::Done).is_ok());
        assert!(validate_transition(&TaskStatus::InProgress, &TaskStatus::Pending).is_ok());
    }

    #[test]
    fn test_invalid_transitions() {
        assert!(validate_transition(&TaskStatus::Pending, &TaskStatus::Done).is_err());
        assert!(validate_transition(&TaskStatus::Done, &TaskStatus::Pending).is_err());
        assert!(validate_transition(&TaskStatus::Done, &TaskStatus::InProgress).is_err());
        assert!(validate_transition(&TaskStatus::Pending, &TaskStatus::Pending).is_err());
    }

    #[test]
    fn test_status_from_str() {
        assert_eq!(TaskStatus::from_str_checked("pending").unwrap(), TaskStatus::Pending);
        assert_eq!(TaskStatus::from_str_checked("in_progress").unwrap(), TaskStatus::InProgress);
        assert_eq!(TaskStatus::from_str_checked("done").unwrap(), TaskStatus::Done);
        assert!(TaskStatus::from_str_checked("invalid").is_err());
    }

    // --- Service tests ---

    fn make_request(title: &str, priority: u8) -> CreateTaskRequest {
        CreateTaskRequest {
            title: title.to_string(),
            description: format!("Description for {title}"),
            priority,
        }
    }

    #[tokio::test]
    async fn test_create_task() {
        let svc = TaskService::new();
        let task = svc.create_task(make_request("Task 1", 3)).await.unwrap();
        assert_eq!(task.id, 1);
        assert_eq!(task.title, "Task 1");
        assert_eq!(task.priority, 3);
        assert_eq!(task.status, TaskStatus::Pending);
    }

    #[tokio::test]
    async fn test_create_task_invalid_priority() {
        let svc = TaskService::new();
        let result = svc.create_task(make_request("Bad", 0)).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Priority"));
    }

    #[tokio::test]
    async fn test_get_task() {
        let svc = TaskService::new();
        let created = svc.create_task(make_request("Find me", 2)).await.unwrap();
        let found = svc.get_task(created.id).await.unwrap();
        assert_eq!(found.title, "Find me");
    }

    #[tokio::test]
    async fn test_get_task_not_found() {
        let svc = TaskService::new();
        assert!(svc.get_task(999).await.is_err());
    }

    #[tokio::test]
    async fn test_list_tasks_sorted() {
        let svc = TaskService::new();
        svc.create_task(make_request("B", 1)).await.unwrap();
        svc.create_task(make_request("A", 2)).await.unwrap();
        svc.create_task(make_request("C", 3)).await.unwrap();

        let tasks = svc.list_tasks().await;
        assert_eq!(tasks.len(), 3);
        assert_eq!(tasks[0].id, 1);
        assert_eq!(tasks[1].id, 2);
        assert_eq!(tasks[2].id, 3);
    }

    #[tokio::test]
    async fn test_update_status_valid() {
        let svc = TaskService::new();
        let task = svc.create_task(make_request("Status test", 1)).await.unwrap();

        let updated = svc.update_status(task.id, "in_progress").await.unwrap();
        assert_eq!(updated.status, TaskStatus::InProgress);

        let done = svc.update_status(task.id, "done").await.unwrap();
        assert_eq!(done.status, TaskStatus::Done);
    }

    #[tokio::test]
    async fn test_update_status_invalid_transition() {
        let svc = TaskService::new();
        let task = svc.create_task(make_request("No skip", 1)).await.unwrap();
        let result = svc.update_status(task.id, "done").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_update_status_not_found() {
        let svc = TaskService::new();
        assert!(svc.update_status(999, "done").await.is_err());
    }

    #[tokio::test]
    async fn test_update_status_invalid_value() {
        let svc = TaskService::new();
        let task = svc.create_task(make_request("Bad status", 1)).await.unwrap();
        let result = svc.update_status(task.id, "garbage").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_delete_task() {
        let svc = TaskService::new();
        let task = svc.create_task(make_request("Delete me", 1)).await.unwrap();
        let deleted = svc.delete_task(task.id).await.unwrap();
        assert_eq!(deleted.title, "Delete me");
        assert!(svc.get_task(task.id).await.is_err());
    }

    #[tokio::test]
    async fn test_delete_task_not_found() {
        let svc = TaskService::new();
        assert!(svc.delete_task(999).await.is_err());
    }

    #[tokio::test]
    async fn test_stats() {
        let svc = TaskService::new();
        svc.create_task(make_request("Low", 1)).await.unwrap();
        svc.create_task(make_request("High", 4)).await.unwrap();
        svc.create_task(make_request("Critical", 5)).await.unwrap();

        // Move one to in_progress
        svc.update_status(2, "in_progress").await.unwrap();
        // Move one to done
        svc.update_status(3, "in_progress").await.unwrap();
        svc.update_status(3, "done").await.unwrap();

        let stats = svc.get_stats().await;
        assert_eq!(stats.total, 3);
        assert_eq!(stats.pending, 1);
        assert_eq!(stats.in_progress, 1);
        assert_eq!(stats.done, 1);
        assert_eq!(stats.high_priority, 2);
    }

    // --- Persistence tests ---

    #[test]
    fn test_persistence_round_trip() {
        let tmp = std::env::temp_dir().join("test_tasks_roundtrip.json");

        let mut tasks = HashMap::new();
        tasks.insert(
            1,
            crate::models::Task {
                id: 1,
                title: "Persisted".to_string(),
                description: "Desc".to_string(),
                priority: 3,
                status: TaskStatus::Pending,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
        );

        persistence::save_tasks(&tmp, &tasks).unwrap();
        let loaded = persistence::load_tasks(&tmp).unwrap();

        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[&1].title, "Persisted");

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn test_load_nonexistent_file() {
        let path = std::path::Path::new("/tmp/does_not_exist_12345.json");
        let loaded = persistence::load_tasks(path).unwrap();
        assert!(loaded.is_empty());
    }

    // --- ID auto-increment test ---

    #[tokio::test]
    async fn test_ids_auto_increment() {
        let svc = TaskService::new();
        let t1 = svc.create_task(make_request("One", 1)).await.unwrap();
        let t2 = svc.create_task(make_request("Two", 2)).await.unwrap();
        let t3 = svc.create_task(make_request("Three", 3)).await.unwrap();
        assert_eq!(t1.id, 1);
        assert_eq!(t2.id, 2);
        assert_eq!(t3.id, 3);
    }

    // --- Load tasks restores counter ---

    #[tokio::test]
    async fn test_load_tasks_restores_counter() {
        let svc = TaskService::new();
        let mut tasks = HashMap::new();
        tasks.insert(
            10,
            crate::models::Task {
                id: 10,
                title: "Loaded".to_string(),
                description: "Desc".to_string(),
                priority: 2,
                status: TaskStatus::Pending,
                created_at: "2026-01-01T00:00:00Z".to_string(),
            },
        );
        svc.load_tasks(tasks).await;

        let new_task = svc.create_task(make_request("After load", 1)).await.unwrap();
        assert_eq!(new_task.id, 11);
    }
}

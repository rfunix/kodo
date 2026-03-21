mod api;
mod models;
mod persistence;
mod service;
#[cfg(test)]
mod tests;

use service::TaskService;

#[tokio::main]
async fn main() {
    let service = TaskService::new();

    // Optionally load persisted tasks
    let data_path = std::path::Path::new("tasks.json");
    if let Ok(tasks) = persistence::load_tasks(data_path) {
        if !tasks.is_empty() {
            println!("Loaded {} tasks from {}", tasks.len(), data_path.display());
            service.load_tasks(tasks).await;
        }
    }

    let app = api::create_router(service);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:8080")
        .await
        .expect("Failed to bind to 0.0.0.0:8080");

    println!("Task Management API running on http://0.0.0.0:8080");

    axum::serve(listener, app)
        .await
        .expect("Server error");
}

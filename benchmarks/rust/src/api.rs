use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post, put},
    Json, Router,
};
use serde_json::json;
use tower_http::cors::{Any, CorsLayer};

use crate::models::{CreateTaskRequest, UpdateStatusRequest};
use crate::service::TaskService;

pub fn create_router(service: TaskService) -> Router {
    let cors = CorsLayer::new()
        .allow_origin(Any)
        .allow_methods(Any)
        .allow_headers(Any);

    Router::new()
        .route("/tasks", post(create_task))
        .route("/tasks", get(list_tasks))
        .route("/tasks/{id}", get(get_task))
        .route("/tasks/{id}/status", put(update_status))
        .route("/tasks/{id}", delete(delete_task))
        .route("/health", get(health))
        .route("/stats", get(get_stats))
        .layer(cors)
        .with_state(service)
}

async fn create_task(
    State(service): State<TaskService>,
    Json(req): Json<CreateTaskRequest>,
) -> impl IntoResponse {
    match service.create_task(req).await {
        Ok(task) => (StatusCode::CREATED, Json(json!(task))).into_response(),
        Err(msg) => (
            StatusCode::BAD_REQUEST,
            Json(json!({ "error": msg })),
        )
            .into_response(),
    }
}

async fn list_tasks(State(service): State<TaskService>) -> impl IntoResponse {
    let tasks = service.list_tasks().await;
    Json(json!(tasks))
}

async fn get_task(
    State(service): State<TaskService>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match service.get_task(id).await {
        Ok(task) => (StatusCode::OK, Json(json!(task))).into_response(),
        Err(msg) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": msg })),
        )
            .into_response(),
    }
}

async fn update_status(
    State(service): State<TaskService>,
    Path(id): Path<u64>,
    Json(req): Json<UpdateStatusRequest>,
) -> impl IntoResponse {
    match service.update_status(id, &req.status).await {
        Ok(task) => (StatusCode::OK, Json(json!(task))).into_response(),
        Err(msg) => {
            let status = if msg.contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::BAD_REQUEST
            };
            (status, Json(json!({ "error": msg }))).into_response()
        }
    }
}

async fn delete_task(
    State(service): State<TaskService>,
    Path(id): Path<u64>,
) -> impl IntoResponse {
    match service.delete_task(id).await {
        Ok(task) => (StatusCode::OK, Json(json!(task))).into_response(),
        Err(msg) => (
            StatusCode::NOT_FOUND,
            Json(json!({ "error": msg })),
        )
            .into_response(),
    }
}

async fn health() -> impl IntoResponse {
    Json(json!({ "status": "ok" }))
}

async fn get_stats(State(service): State<TaskService>) -> impl IntoResponse {
    let stats = service.get_stats().await;
    Json(json!(stats))
}

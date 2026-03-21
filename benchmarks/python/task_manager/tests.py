import pytest
from fastapi.testclient import TestClient

from .api import app, service
from .models import TaskStatus
from .service import TaskService, validate_transition


@pytest.fixture(autouse=True)
def _reset_service():
    """Reset the global service before each test."""
    service._tasks.clear()
    service._next_id = 1
    yield


client = TestClient(app)


# --- Priority validation ---


def test_create_task_valid_priority():
    for prio in (1, 3, 5):
        resp = client.post("/tasks", json={"title": "t", "priority": prio})
        assert resp.status_code == 201
        assert resp.json()["priority"] == prio


def test_create_task_invalid_priority_zero():
    resp = client.post("/tasks", json={"title": "t", "priority": 0})
    assert resp.status_code == 422


def test_create_task_invalid_priority_six():
    resp = client.post("/tasks", json={"title": "t", "priority": 6})
    assert resp.status_code == 422


# --- Status transitions ---


def test_valid_transition_pending_to_in_progress():
    assert validate_transition(TaskStatus.PENDING, TaskStatus.IN_PROGRESS) is True


def test_valid_transition_in_progress_to_done():
    assert validate_transition(TaskStatus.IN_PROGRESS, TaskStatus.DONE) is True


def test_invalid_transition_pending_to_done():
    assert validate_transition(TaskStatus.PENDING, TaskStatus.DONE) is False


def test_invalid_transition_done_to_pending():
    assert validate_transition(TaskStatus.DONE, TaskStatus.PENDING) is False


def test_update_status_valid_via_api():
    client.post("/tasks", json={"title": "t", "priority": 1})
    resp = client.put("/tasks/1/status", json={"status": "in_progress"})
    assert resp.status_code == 200
    assert resp.json()["status"] == "in_progress"


def test_update_status_invalid_via_api():
    client.post("/tasks", json={"title": "t", "priority": 1})
    resp = client.put("/tasks/1/status", json={"status": "done"})
    assert resp.status_code == 400


# --- List tasks ---


def test_list_tasks_empty():
    resp = client.get("/tasks")
    assert resp.status_code == 200
    assert resp.json() == []


def test_list_tasks_after_creation():
    client.post("/tasks", json={"title": "a", "priority": 1})
    client.post("/tasks", json={"title": "b", "priority": 2})
    resp = client.get("/tasks")
    assert len(resp.json()) == 2


# --- Get task ---


def test_get_task_existing():
    client.post("/tasks", json={"title": "x", "priority": 3})
    resp = client.get("/tasks/1")
    assert resp.status_code == 200
    assert resp.json()["title"] == "x"


def test_get_task_not_found():
    resp = client.get("/tasks/999")
    assert resp.status_code == 404


# --- Delete task ---


def test_delete_task_existing():
    client.post("/tasks", json={"title": "d", "priority": 1})
    resp = client.delete("/tasks/1")
    assert resp.status_code == 200
    assert client.get("/tasks/1").status_code == 404


def test_delete_task_not_found():
    resp = client.delete("/tasks/999")
    assert resp.status_code == 404


# --- Stats ---


def test_stats_calculation():
    client.post("/tasks", json={"title": "a", "priority": 1})
    client.post("/tasks", json={"title": "b", "priority": 4})
    client.post("/tasks", json={"title": "c", "priority": 5})
    # move task 2 to in_progress
    client.put("/tasks/2/status", json={"status": "in_progress"})
    # move task 3 to in_progress then done
    client.put("/tasks/3/status", json={"status": "in_progress"})
    client.put("/tasks/3/status", json={"status": "done"})

    resp = client.get("/stats")
    assert resp.status_code == 200
    data = resp.json()
    assert data["total"] == 3
    assert data["pending"] == 1
    assert data["in_progress"] == 1
    assert data["done"] == 1
    assert data["high_priority"] == 2


# --- Health ---


def test_health():
    resp = client.get("/health")
    assert resp.status_code == 200
    assert resp.json() == {"status": "ok"}

from fastapi import FastAPI, HTTPException

from .models import TaskCreate, TaskUpdate
from .service import TaskService

app = FastAPI(title="Task Manager API")
service = TaskService()


@app.post("/tasks", status_code=201)
def create_task(data: TaskCreate):
    task = service.create_task(data)
    return task.model_dump()


@app.get("/tasks")
def list_tasks():
    return [t.model_dump() for t in service.list_tasks()]


@app.get("/tasks/{task_id}")
def get_task(task_id: int):
    task = service.get_task(task_id)
    if task is None:
        raise HTTPException(status_code=404, detail=f"Task {task_id} not found")
    return task.model_dump()


@app.put("/tasks/{task_id}/status")
def update_status(task_id: int, data: TaskUpdate):
    try:
        task = service.update_status(task_id, data.status)
    except KeyError:
        raise HTTPException(status_code=404, detail=f"Task {task_id} not found")
    except ValueError as exc:
        raise HTTPException(status_code=400, detail=str(exc))
    return task.model_dump()


@app.delete("/tasks/{task_id}")
def delete_task(task_id: int):
    if not service.delete_task(task_id):
        raise HTTPException(status_code=404, detail=f"Task {task_id} not found")
    return {"detail": "deleted"}


@app.get("/health")
def health():
    return {"status": "ok"}


@app.get("/stats")
def stats():
    return service.get_stats()

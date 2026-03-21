from datetime import datetime, timezone

from .models import Task, TaskCreate, TaskStatus, VALID_TRANSITIONS


def validate_transition(current: TaskStatus, next_status: TaskStatus) -> bool:
    """Check whether a status transition is allowed."""
    return next_status in VALID_TRANSITIONS.get(current, [])


class TaskService:
    def __init__(self) -> None:
        self._tasks: dict[int, Task] = {}
        self._next_id: int = 1

    def create_task(self, data: TaskCreate) -> Task:
        task = Task(
            id=self._next_id,
            title=data.title,
            description=data.description,
            priority=data.priority,
            status=TaskStatus.PENDING,
            created_at=datetime.now(timezone.utc).isoformat(),
        )
        self._tasks[task.id] = task
        self._next_id += 1
        return task

    def get_task(self, task_id: int) -> Task | None:
        return self._tasks.get(task_id)

    def list_tasks(self) -> list[Task]:
        return list(self._tasks.values())

    def update_status(self, task_id: int, new_status_str: str) -> Task:
        task = self._tasks.get(task_id)
        if task is None:
            raise KeyError(f"Task {task_id} not found")

        try:
            new_status = TaskStatus(new_status_str)
        except ValueError:
            raise ValueError(f"Invalid status: {new_status_str}")

        if not validate_transition(task.status, new_status):
            raise ValueError(
                f"Invalid transition: {task.status.value} -> {new_status.value}"
            )

        updated = task.model_copy(update={"status": new_status})
        self._tasks[task_id] = updated
        return updated

    def delete_task(self, task_id: int) -> bool:
        if task_id in self._tasks:
            del self._tasks[task_id]
            return True
        return False

    def get_stats(self) -> dict[str, int]:
        tasks = list(self._tasks.values())
        return {
            "total": len(tasks),
            "pending": sum(1 for t in tasks if t.status == TaskStatus.PENDING),
            "in_progress": sum(1 for t in tasks if t.status == TaskStatus.IN_PROGRESS),
            "done": sum(1 for t in tasks if t.status == TaskStatus.DONE),
            "high_priority": sum(1 for t in tasks if t.priority >= 4),
        }

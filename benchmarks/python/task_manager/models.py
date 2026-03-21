from enum import Enum

from pydantic import BaseModel, Field


class TaskStatus(str, Enum):
    PENDING = "pending"
    IN_PROGRESS = "in_progress"
    DONE = "done"


VALID_TRANSITIONS: dict[TaskStatus, list[TaskStatus]] = {
    TaskStatus.PENDING: [TaskStatus.IN_PROGRESS],
    TaskStatus.IN_PROGRESS: [TaskStatus.DONE],
    TaskStatus.DONE: [],
}


class Task(BaseModel):
    id: int
    title: str = Field(min_length=1)
    description: str
    priority: int = Field(ge=1, le=5)
    status: TaskStatus
    created_at: str


class TaskCreate(BaseModel):
    title: str = Field(min_length=1)
    description: str = ""
    priority: int = Field(ge=1, le=5)


class TaskUpdate(BaseModel):
    status: str

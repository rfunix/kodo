import { Task, TaskCreate, TaskStatus, VALID_TRANSITIONS } from "./models.js";

export interface TaskStats {
  total: number;
  pending: number;
  in_progress: number;
  done: number;
  high_priority: number;
}

export class InvalidTransitionError extends Error {
  constructor(from: TaskStatus, to: TaskStatus) {
    super(`Invalid transition from '${from}' to '${to}'`);
    this.name = "InvalidTransitionError";
  }
}

export class TaskNotFoundError extends Error {
  constructor(id: number) {
    super(`Task with id ${id} not found`);
    this.name = "TaskNotFoundError";
  }
}

export function validateTransition(from: TaskStatus, to: TaskStatus): boolean {
  const allowed = VALID_TRANSITIONS.get(from);
  if (!allowed) return false;
  return allowed.includes(to);
}

function parseStatus(value: string): TaskStatus | null {
  const values = Object.values(TaskStatus) as string[];
  if (values.includes(value)) {
    return value as TaskStatus;
  }
  return null;
}

export class TaskService {
  private tasks: Map<number, Task> = new Map();
  private nextId: number = 1;

  createTask(data: TaskCreate): Task {
    const task: Task = {
      id: this.nextId++,
      title: data.title,
      description: data.description,
      priority: data.priority,
      status: TaskStatus.Pending,
      created_at: new Date().toISOString(),
    };
    this.tasks.set(task.id, task);
    return task;
  }

  getTask(id: number): Task {
    const task = this.tasks.get(id);
    if (!task) {
      throw new TaskNotFoundError(id);
    }
    return task;
  }

  listTasks(): Task[] {
    return Array.from(this.tasks.values());
  }

  updateStatus(id: number, statusStr: string): Task {
    const task = this.getTask(id);
    const newStatus = parseStatus(statusStr);
    if (!newStatus) {
      throw new InvalidTransitionError(task.status, statusStr as TaskStatus);
    }
    if (!validateTransition(task.status, newStatus)) {
      throw new InvalidTransitionError(task.status, newStatus);
    }
    task.status = newStatus;
    return task;
  }

  deleteTask(id: number): Task {
    const task = this.getTask(id);
    this.tasks.delete(id);
    return task;
  }

  getStats(): TaskStats {
    const tasks = this.listTasks();
    return {
      total: tasks.length,
      pending: tasks.filter((t) => t.status === TaskStatus.Pending).length,
      in_progress: tasks.filter((t) => t.status === TaskStatus.InProgress).length,
      done: tasks.filter((t) => t.status === TaskStatus.Done).length,
      high_priority: tasks.filter((t) => t.priority >= 4).length,
    };
  }
}

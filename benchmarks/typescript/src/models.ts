import { z } from "zod";

export enum TaskStatus {
  Pending = "pending",
  InProgress = "in_progress",
  Done = "done",
}

export interface Task {
  id: number;
  title: string;
  description: string;
  priority: number;
  status: TaskStatus;
  created_at: string;
}

export const TaskCreateSchema = z.object({
  title: z.string().min(1, "Title must have at least 1 character"),
  description: z.string().default(""),
  priority: z.number().int().min(1, "Priority must be between 1 and 5").max(5, "Priority must be between 1 and 5"),
});

export type TaskCreate = z.infer<typeof TaskCreateSchema>;

export const TaskUpdateSchema = z.object({
  status: z.string(),
});

export type TaskUpdate = z.infer<typeof TaskUpdateSchema>;

export const VALID_TRANSITIONS: ReadonlyMap<TaskStatus, readonly TaskStatus[]> = new Map([
  [TaskStatus.Pending, [TaskStatus.InProgress] as const],
  [TaskStatus.InProgress, [TaskStatus.Done] as const],
  [TaskStatus.Done, [] as const],
]);

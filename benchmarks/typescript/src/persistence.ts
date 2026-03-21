import { readFileSync, writeFileSync } from "node:fs";
import type { Task } from "./models.js";

export function saveTasksToFile(tasks: Task[], filePath: string): void {
  const json = JSON.stringify(tasks, null, 2);
  writeFileSync(filePath, json, "utf-8");
}

export function loadTasksFromFile(filePath: string): Task[] {
  const content = readFileSync(filePath, "utf-8");
  return JSON.parse(content) as Task[];
}

import { Router, type Request, type Response } from "express";
import { ZodError } from "zod";
import { TaskCreateSchema, TaskUpdateSchema } from "./models.js";
import { TaskService, TaskNotFoundError, InvalidTransitionError } from "./service.js";

export function createRouter(service: TaskService): Router {
  const router = Router();

  router.post("/tasks", (req: Request, res: Response) => {
    try {
      const data = TaskCreateSchema.parse(req.body);
      const task = service.createTask(data);
      res.status(201).json(task);
    } catch (err) {
      if (err instanceof ZodError) {
        res.status(422).json({ error: "Validation error", details: err.errors });
        return;
      }
      res.status(500).json({ error: "Internal server error" });
    }
  });

  router.get("/tasks", (_req: Request, res: Response) => {
    const tasks = service.listTasks();
    res.json(tasks);
  });

  router.get("/tasks/:id", (req: Request, res: Response) => {
    try {
      const id = parseInt(req.params.id, 10);
      if (isNaN(id)) {
        res.status(400).json({ error: "Invalid task ID" });
        return;
      }
      const task = service.getTask(id);
      res.json(task);
    } catch (err) {
      if (err instanceof TaskNotFoundError) {
        res.status(404).json({ error: err.message });
        return;
      }
      res.status(500).json({ error: "Internal server error" });
    }
  });

  router.put("/tasks/:id/status", (req: Request, res: Response) => {
    try {
      const id = parseInt(req.params.id, 10);
      if (isNaN(id)) {
        res.status(400).json({ error: "Invalid task ID" });
        return;
      }
      const data = TaskUpdateSchema.parse(req.body);
      const task = service.updateStatus(id, data.status);
      res.json(task);
    } catch (err) {
      if (err instanceof ZodError) {
        res.status(422).json({ error: "Validation error", details: err.errors });
        return;
      }
      if (err instanceof TaskNotFoundError) {
        res.status(404).json({ error: err.message });
        return;
      }
      if (err instanceof InvalidTransitionError) {
        res.status(400).json({ error: err.message });
        return;
      }
      res.status(500).json({ error: "Internal server error" });
    }
  });

  router.delete("/tasks/:id", (req: Request, res: Response) => {
    try {
      const id = parseInt(req.params.id, 10);
      if (isNaN(id)) {
        res.status(400).json({ error: "Invalid task ID" });
        return;
      }
      const task = service.deleteTask(id);
      res.json(task);
    } catch (err) {
      if (err instanceof TaskNotFoundError) {
        res.status(404).json({ error: err.message });
        return;
      }
      res.status(500).json({ error: "Internal server error" });
    }
  });

  router.get("/health", (_req: Request, res: Response) => {
    res.json({ status: "ok" });
  });

  router.get("/stats", (_req: Request, res: Response) => {
    const stats = service.getStats();
    res.json(stats);
  });

  return router;
}

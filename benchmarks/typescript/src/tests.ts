import { describe, it, expect, beforeEach } from "vitest";
import express from "express";
import request from "supertest";
import { createRouter } from "./api.js";
import { TaskService, validateTransition, InvalidTransitionError, TaskNotFoundError } from "./service.js";
import { TaskStatus, VALID_TRANSITIONS } from "./models.js";

// --- Unit Tests: Models ---

describe("Models", () => {
  it("TaskStatus has correct values", () => {
    expect(TaskStatus.Pending).toBe("pending");
    expect(TaskStatus.InProgress).toBe("in_progress");
    expect(TaskStatus.Done).toBe("done");
  });

  it("VALID_TRANSITIONS allows pending -> in_progress", () => {
    const allowed = VALID_TRANSITIONS.get(TaskStatus.Pending);
    expect(allowed).toContain(TaskStatus.InProgress);
  });

  it("VALID_TRANSITIONS disallows done -> any", () => {
    const allowed = VALID_TRANSITIONS.get(TaskStatus.Done);
    expect(allowed).toEqual([]);
  });
});

// --- Unit Tests: Service ---

describe("TaskService", () => {
  let service: TaskService;

  beforeEach(() => {
    service = new TaskService();
  });

  describe("createTask", () => {
    it("creates a task with correct defaults", () => {
      const task = service.createTask({ title: "Test", description: "", priority: 3 });
      expect(task.id).toBe(1);
      expect(task.title).toBe("Test");
      expect(task.status).toBe(TaskStatus.Pending);
      expect(task.priority).toBe(3);
      expect(task.created_at).toBeTruthy();
    });

    it("auto-increments IDs", () => {
      const t1 = service.createTask({ title: "A", description: "", priority: 1 });
      const t2 = service.createTask({ title: "B", description: "", priority: 2 });
      expect(t2.id).toBe(t1.id + 1);
    });
  });

  describe("getTask", () => {
    it("returns existing task", () => {
      const created = service.createTask({ title: "Find me", description: "", priority: 1 });
      const found = service.getTask(created.id);
      expect(found.title).toBe("Find me");
    });

    it("throws TaskNotFoundError for missing task", () => {
      expect(() => service.getTask(999)).toThrow(TaskNotFoundError);
    });
  });

  describe("listTasks", () => {
    it("returns empty array initially", () => {
      expect(service.listTasks()).toEqual([]);
    });

    it("returns all created tasks", () => {
      service.createTask({ title: "A", description: "", priority: 1 });
      service.createTask({ title: "B", description: "", priority: 2 });
      expect(service.listTasks()).toHaveLength(2);
    });
  });

  describe("updateStatus", () => {
    it("allows valid transition pending -> in_progress", () => {
      const task = service.createTask({ title: "Test", description: "", priority: 1 });
      const updated = service.updateStatus(task.id, "in_progress");
      expect(updated.status).toBe(TaskStatus.InProgress);
    });

    it("allows valid transition in_progress -> done", () => {
      const task = service.createTask({ title: "Test", description: "", priority: 1 });
      service.updateStatus(task.id, "in_progress");
      const updated = service.updateStatus(task.id, "done");
      expect(updated.status).toBe(TaskStatus.Done);
    });

    it("rejects invalid transition pending -> done", () => {
      const task = service.createTask({ title: "Test", description: "", priority: 1 });
      expect(() => service.updateStatus(task.id, "done")).toThrow(InvalidTransitionError);
    });

    it("rejects invalid transition done -> pending", () => {
      const task = service.createTask({ title: "Test", description: "", priority: 1 });
      service.updateStatus(task.id, "in_progress");
      service.updateStatus(task.id, "done");
      expect(() => service.updateStatus(task.id, "pending")).toThrow(InvalidTransitionError);
    });

    it("rejects invalid status string", () => {
      const task = service.createTask({ title: "Test", description: "", priority: 1 });
      expect(() => service.updateStatus(task.id, "invalid_status")).toThrow(InvalidTransitionError);
    });
  });

  describe("deleteTask", () => {
    it("removes and returns the task", () => {
      const task = service.createTask({ title: "Delete me", description: "", priority: 1 });
      const deleted = service.deleteTask(task.id);
      expect(deleted.title).toBe("Delete me");
      expect(service.listTasks()).toHaveLength(0);
    });

    it("throws TaskNotFoundError for missing task", () => {
      expect(() => service.deleteTask(999)).toThrow(TaskNotFoundError);
    });
  });

  describe("getStats", () => {
    it("returns zeros for empty service", () => {
      const stats = service.getStats();
      expect(stats).toEqual({
        total: 0,
        pending: 0,
        in_progress: 0,
        done: 0,
        high_priority: 0,
      });
    });

    it("calculates correct stats", () => {
      service.createTask({ title: "Low", description: "", priority: 1 });
      service.createTask({ title: "High", description: "", priority: 4 });
      service.createTask({ title: "Critical", description: "", priority: 5 });
      const task4 = service.createTask({ title: "Moving", description: "", priority: 2 });
      service.updateStatus(task4.id, "in_progress");
      service.updateStatus(task4.id, "done");

      const stats = service.getStats();
      expect(stats.total).toBe(4);
      expect(stats.pending).toBe(3);
      expect(stats.in_progress).toBe(0);
      expect(stats.done).toBe(1);
      expect(stats.high_priority).toBe(2);
    });
  });

  describe("validateTransition", () => {
    it("returns true for valid transitions", () => {
      expect(validateTransition(TaskStatus.Pending, TaskStatus.InProgress)).toBe(true);
      expect(validateTransition(TaskStatus.InProgress, TaskStatus.Done)).toBe(true);
    });

    it("returns false for invalid transitions", () => {
      expect(validateTransition(TaskStatus.Pending, TaskStatus.Done)).toBe(false);
      expect(validateTransition(TaskStatus.Done, TaskStatus.Pending)).toBe(false);
      expect(validateTransition(TaskStatus.Done, TaskStatus.InProgress)).toBe(false);
    });
  });
});

// --- Integration Tests: API ---

describe("API", () => {
  let app: express.Express;
  let service: TaskService;

  beforeEach(() => {
    service = new TaskService();
    app = express();
    app.use(express.json());
    app.use(createRouter(service));
  });

  describe("POST /tasks", () => {
    it("creates a task with valid data", async () => {
      const res = await request(app)
        .post("/tasks")
        .send({ title: "New task", description: "A description", priority: 3 });
      expect(res.status).toBe(201);
      expect(res.body.title).toBe("New task");
      expect(res.body.status).toBe("pending");
    });

    it("rejects task with invalid priority (too high)", async () => {
      const res = await request(app)
        .post("/tasks")
        .send({ title: "Bad", description: "", priority: 10 });
      expect(res.status).toBe(422);
    });

    it("rejects task with invalid priority (too low)", async () => {
      const res = await request(app)
        .post("/tasks")
        .send({ title: "Bad", description: "", priority: 0 });
      expect(res.status).toBe(422);
    });

    it("rejects task with empty title", async () => {
      const res = await request(app)
        .post("/tasks")
        .send({ title: "", description: "", priority: 1 });
      expect(res.status).toBe(422);
    });

    it("uses default empty description", async () => {
      const res = await request(app)
        .post("/tasks")
        .send({ title: "Minimal", priority: 2 });
      expect(res.status).toBe(201);
      expect(res.body.description).toBe("");
    });
  });

  describe("GET /tasks", () => {
    it("returns empty list initially", async () => {
      const res = await request(app).get("/tasks");
      expect(res.status).toBe(200);
      expect(res.body).toEqual([]);
    });

    it("returns all tasks", async () => {
      await request(app).post("/tasks").send({ title: "A", priority: 1 });
      await request(app).post("/tasks").send({ title: "B", priority: 2 });
      const res = await request(app).get("/tasks");
      expect(res.body).toHaveLength(2);
    });
  });

  describe("GET /tasks/:id", () => {
    it("returns a task by ID", async () => {
      const created = await request(app).post("/tasks").send({ title: "Find me", priority: 1 });
      const res = await request(app).get(`/tasks/${created.body.id}`);
      expect(res.status).toBe(200);
      expect(res.body.title).toBe("Find me");
    });

    it("returns 404 for missing task", async () => {
      const res = await request(app).get("/tasks/999");
      expect(res.status).toBe(404);
    });
  });

  describe("PUT /tasks/:id/status", () => {
    it("updates status with valid transition", async () => {
      const created = await request(app).post("/tasks").send({ title: "Move me", priority: 1 });
      const res = await request(app)
        .put(`/tasks/${created.body.id}/status`)
        .send({ status: "in_progress" });
      expect(res.status).toBe(200);
      expect(res.body.status).toBe("in_progress");
    });

    it("rejects invalid transition", async () => {
      const created = await request(app).post("/tasks").send({ title: "Stuck", priority: 1 });
      const res = await request(app)
        .put(`/tasks/${created.body.id}/status`)
        .send({ status: "done" });
      expect(res.status).toBe(400);
    });

    it("returns 404 for missing task", async () => {
      const res = await request(app)
        .put("/tasks/999/status")
        .send({ status: "in_progress" });
      expect(res.status).toBe(404);
    });
  });

  describe("DELETE /tasks/:id", () => {
    it("deletes existing task", async () => {
      const created = await request(app).post("/tasks").send({ title: "Delete me", priority: 1 });
      const res = await request(app).delete(`/tasks/${created.body.id}`);
      expect(res.status).toBe(200);

      const check = await request(app).get(`/tasks/${created.body.id}`);
      expect(check.status).toBe(404);
    });

    it("returns 404 for missing task", async () => {
      const res = await request(app).delete("/tasks/999");
      expect(res.status).toBe(404);
    });
  });

  describe("GET /health", () => {
    it("returns ok status", async () => {
      const res = await request(app).get("/health");
      expect(res.status).toBe(200);
      expect(res.body).toEqual({ status: "ok" });
    });
  });

  describe("GET /stats", () => {
    it("returns correct stats", async () => {
      await request(app).post("/tasks").send({ title: "A", priority: 1 });
      await request(app).post("/tasks").send({ title: "B", priority: 5 });
      const created = await request(app).post("/tasks").send({ title: "C", priority: 3 });
      await request(app)
        .put(`/tasks/${created.body.id}/status`)
        .send({ status: "in_progress" });

      const res = await request(app).get("/stats");
      expect(res.status).toBe(200);
      expect(res.body.total).toBe(3);
      expect(res.body.pending).toBe(2);
      expect(res.body.in_progress).toBe(1);
      expect(res.body.done).toBe(0);
      expect(res.body.high_priority).toBe(1);
    });
  });
});

import express from "express";
import { createRouter } from "./api.js";
import { TaskService } from "./service.js";

const app = express();
const service = new TaskService();

app.use(express.json());
app.use(createRouter(service));

const PORT = 8080;

app.listen(PORT, () => {
  console.log(`Task Manager API running on http://localhost:${PORT}`);
});

export { app, service };

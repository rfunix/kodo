package main

import (
	"encoding/json"
	"fmt"
	"net/http"
	"strconv"
	"strings"
)

func writeJSON(w http.ResponseWriter, status int, data any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	json.NewEncoder(w).Encode(data)
}

func writeError(w http.ResponseWriter, status int, message string) {
	writeJSON(w, status, map[string]string{"error": message})
}

func SetupRoutes(svc *TaskService) *http.ServeMux {
	mux := http.NewServeMux()

	mux.HandleFunc("POST /tasks", handleCreateTask(svc))
	mux.HandleFunc("GET /tasks", handleListTasks(svc))
	mux.HandleFunc("GET /tasks/{id}", handleGetTask(svc))
	mux.HandleFunc("PUT /tasks/{id}/status", handleUpdateStatus(svc))
	mux.HandleFunc("DELETE /tasks/{id}", handleDeleteTask(svc))
	mux.HandleFunc("GET /health", handleHealth())
	mux.HandleFunc("GET /stats", handleStats(svc))

	return mux
}

func handleCreateTask(svc *TaskService) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		var req CreateTaskRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeError(w, http.StatusBadRequest, "invalid request body")
			return
		}

		task, err := svc.CreateTask(req)
		if err != nil {
			writeError(w, http.StatusBadRequest, err.Error())
			return
		}

		writeJSON(w, http.StatusCreated, task)
	}
}

func handleListTasks(svc *TaskService) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		tasks := svc.ListTasks()
		writeJSON(w, http.StatusOK, tasks)
	}
}

func handleGetTask(svc *TaskService) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		id, err := parseTaskID(r)
		if err != nil {
			writeError(w, http.StatusBadRequest, err.Error())
			return
		}

		task, err := svc.GetTask(id)
		if err != nil {
			writeError(w, http.StatusNotFound, err.Error())
			return
		}

		writeJSON(w, http.StatusOK, task)
	}
}

func handleUpdateStatus(svc *TaskService) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		// Extract ID from path, stripping "/status" suffix
		idStr := r.PathValue("id")
		idStr = strings.TrimSuffix(idStr, "/status")

		id, err := strconv.ParseUint(idStr, 10, 64)
		if err != nil {
			writeError(w, http.StatusBadRequest, "invalid task id")
			return
		}

		var req UpdateStatusRequest
		if err := json.NewDecoder(r.Body).Decode(&req); err != nil {
			writeError(w, http.StatusBadRequest, "invalid request body")
			return
		}

		status, err := StatusFromString(req.Status)
		if err != nil {
			writeError(w, http.StatusBadRequest, err.Error())
			return
		}

		task, err := svc.UpdateStatus(id, status)
		if err != nil {
			if strings.Contains(err.Error(), "not found") {
				writeError(w, http.StatusNotFound, err.Error())
			} else {
				writeError(w, http.StatusBadRequest, err.Error())
			}
			return
		}

		writeJSON(w, http.StatusOK, task)
	}
}

func handleDeleteTask(svc *TaskService) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		id, err := parseTaskID(r)
		if err != nil {
			writeError(w, http.StatusBadRequest, err.Error())
			return
		}

		if err := svc.DeleteTask(id); err != nil {
			writeError(w, http.StatusNotFound, err.Error())
			return
		}

		w.WriteHeader(http.StatusNoContent)
	}
}

func handleHealth() http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		writeJSON(w, http.StatusOK, map[string]string{"status": "ok"})
	}
}

func handleStats(svc *TaskService) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		stats := svc.GetStats()
		writeJSON(w, http.StatusOK, stats)
	}
}

func parseTaskID(r *http.Request) (uint64, error) {
	idStr := r.PathValue("id")
	id, err := strconv.ParseUint(idStr, 10, 64)
	if err != nil {
		return 0, fmt.Errorf("invalid task id")
	}
	return id, nil
}

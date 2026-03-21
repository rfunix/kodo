package main

import (
	"fmt"
	"strings"
)

type TaskStatus int

const (
	StatusPending    TaskStatus = iota
	StatusInProgress
	StatusDone
)

type Task struct {
	ID          uint64     `json:"id"`
	Title       string     `json:"title"`
	Description string     `json:"description"`
	Priority    int        `json:"priority"`
	Status      TaskStatus `json:"status"`
	CreatedAt   string     `json:"created_at"`
}

type CreateTaskRequest struct {
	Title       string `json:"title"`
	Description string `json:"description"`
	Priority    int    `json:"priority"`
}

type UpdateStatusRequest struct {
	Status string `json:"status"`
}

func ValidatePriority(p int) error {
	if p < 1 || p > 5 {
		return fmt.Errorf("priority must be between 1 and 5, got %d", p)
	}
	return nil
}

func ValidateTransition(current, next TaskStatus) error {
	switch current {
	case StatusPending:
		if next != StatusInProgress {
			return fmt.Errorf("pending tasks can only transition to in_progress")
		}
	case StatusInProgress:
		if next != StatusDone && next != StatusPending {
			return fmt.Errorf("in_progress tasks can only transition to done or pending")
		}
	case StatusDone:
		return fmt.Errorf("done tasks cannot transition to another status")
	}
	return nil
}

func StatusFromString(s string) (TaskStatus, error) {
	switch strings.ToLower(s) {
	case "pending":
		return StatusPending, nil
	case "in_progress":
		return StatusInProgress, nil
	case "done":
		return StatusDone, nil
	default:
		return 0, fmt.Errorf("unknown status: %s", s)
	}
}

func StatusToString(s TaskStatus) string {
	switch s {
	case StatusPending:
		return "pending"
	case StatusInProgress:
		return "in_progress"
	case StatusDone:
		return "done"
	default:
		return "unknown"
	}
}

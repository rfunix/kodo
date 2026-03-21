package main

import (
	"fmt"
	"sync"
	"time"
)

type Stats struct {
	Total        int `json:"total"`
	Pending      int `json:"pending"`
	InProgress   int `json:"in_progress"`
	Done         int `json:"done"`
	HighPriority int `json:"high_priority"`
}

type TaskService struct {
	tasks  map[uint64]*Task
	nextID uint64
	mu     sync.RWMutex
}

func NewTaskService() *TaskService {
	return &TaskService{
		tasks:  make(map[uint64]*Task),
		nextID: 1,
	}
}

func (s *TaskService) CreateTask(req CreateTaskRequest) (*Task, error) {
	if err := ValidatePriority(req.Priority); err != nil {
		return nil, err
	}

	if req.Title == "" {
		return nil, fmt.Errorf("title is required")
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	task := &Task{
		ID:          s.nextID,
		Title:       req.Title,
		Description: req.Description,
		Priority:    req.Priority,
		Status:      StatusPending,
		CreatedAt:   time.Now().UTC().Format(time.RFC3339),
	}

	s.tasks[s.nextID] = task
	s.nextID++

	return task, nil
}

func (s *TaskService) GetTask(id uint64) (*Task, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	task, ok := s.tasks[id]
	if !ok {
		return nil, fmt.Errorf("task %d not found", id)
	}

	return task, nil
}

func (s *TaskService) ListTasks() []*Task {
	s.mu.RLock()
	defer s.mu.RUnlock()

	result := make([]*Task, 0, len(s.tasks))
	for _, task := range s.tasks {
		result = append(result, task)
	}

	return result
}

func (s *TaskService) UpdateStatus(id uint64, newStatus TaskStatus) (*Task, error) {
	s.mu.Lock()
	defer s.mu.Unlock()

	task, ok := s.tasks[id]
	if !ok {
		return nil, fmt.Errorf("task %d not found", id)
	}

	if err := ValidateTransition(task.Status, newStatus); err != nil {
		return nil, err
	}

	task.Status = newStatus

	return task, nil
}

func (s *TaskService) DeleteTask(id uint64) error {
	s.mu.Lock()
	defer s.mu.Unlock()

	if _, ok := s.tasks[id]; !ok {
		return fmt.Errorf("task %d not found", id)
	}

	delete(s.tasks, id)

	return nil
}

func (s *TaskService) GetStats() Stats {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var stats Stats
	stats.Total = len(s.tasks)

	for _, task := range s.tasks {
		switch task.Status {
		case StatusPending:
			stats.Pending++
		case StatusInProgress:
			stats.InProgress++
		case StatusDone:
			stats.Done++
		}

		if task.Priority >= 4 {
			stats.HighPriority++
		}
	}

	return stats
}

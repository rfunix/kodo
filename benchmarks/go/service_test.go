package main

import (
	"testing"
)

func TestCreateTask(t *testing.T) {
	tests := []struct {
		name    string
		req     CreateTaskRequest
		wantErr bool
	}{
		{
			name:    "valid task priority 1",
			req:     CreateTaskRequest{Title: "Task 1", Description: "Desc", Priority: 1},
			wantErr: false,
		},
		{
			name:    "valid task priority 5",
			req:     CreateTaskRequest{Title: "Task 5", Description: "Desc", Priority: 5},
			wantErr: false,
		},
		{
			name:    "invalid priority 0",
			req:     CreateTaskRequest{Title: "Task", Description: "Desc", Priority: 0},
			wantErr: true,
		},
		{
			name:    "invalid priority 6",
			req:     CreateTaskRequest{Title: "Task", Description: "Desc", Priority: 6},
			wantErr: true,
		},
		{
			name:    "invalid priority negative",
			req:     CreateTaskRequest{Title: "Task", Description: "Desc", Priority: -1},
			wantErr: true,
		},
		{
			name:    "empty title",
			req:     CreateTaskRequest{Title: "", Description: "Desc", Priority: 3},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			svc := NewTaskService()
			task, err := svc.CreateTask(tt.req)

			if tt.wantErr {
				if err == nil {
					t.Errorf("expected error, got nil")
				}
				return
			}

			if err != nil {
				t.Errorf("unexpected error: %v", err)
				return
			}

			if task.Title != tt.req.Title {
				t.Errorf("title = %q, want %q", task.Title, tt.req.Title)
			}
			if task.Priority != tt.req.Priority {
				t.Errorf("priority = %d, want %d", task.Priority, tt.req.Priority)
			}
			if task.Status != StatusPending {
				t.Errorf("status = %d, want %d (pending)", task.Status, StatusPending)
			}
		})
	}
}

func TestStatusTransitions(t *testing.T) {
	tests := []struct {
		name    string
		current TaskStatus
		next    TaskStatus
		wantErr bool
	}{
		{
			name:    "pending to in_progress",
			current: StatusPending,
			next:    StatusInProgress,
			wantErr: false,
		},
		{
			name:    "pending to done (invalid)",
			current: StatusPending,
			next:    StatusDone,
			wantErr: true,
		},
		{
			name:    "in_progress to done",
			current: StatusInProgress,
			next:    StatusDone,
			wantErr: false,
		},
		{
			name:    "in_progress to pending",
			current: StatusInProgress,
			next:    StatusPending,
			wantErr: false,
		},
		{
			name:    "done to pending (invalid)",
			current: StatusDone,
			next:    StatusPending,
			wantErr: true,
		},
		{
			name:    "done to in_progress (invalid)",
			current: StatusDone,
			next:    StatusInProgress,
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateTransition(tt.current, tt.next)
			if tt.wantErr && err == nil {
				t.Errorf("expected error, got nil")
			}
			if !tt.wantErr && err != nil {
				t.Errorf("unexpected error: %v", err)
			}
		})
	}
}

func TestUpdateStatus(t *testing.T) {
	svc := NewTaskService()
	task, err := svc.CreateTask(CreateTaskRequest{
		Title:    "Test Task",
		Priority: 3,
	})
	if err != nil {
		t.Fatalf("failed to create task: %v", err)
	}

	// pending -> in_progress
	updated, err := svc.UpdateStatus(task.ID, StatusInProgress)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if updated.Status != StatusInProgress {
		t.Errorf("status = %d, want %d", updated.Status, StatusInProgress)
	}

	// in_progress -> done
	updated, err = svc.UpdateStatus(task.ID, StatusDone)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if updated.Status != StatusDone {
		t.Errorf("status = %d, want %d", updated.Status, StatusDone)
	}

	// done -> pending (invalid)
	_, err = svc.UpdateStatus(task.ID, StatusPending)
	if err == nil {
		t.Errorf("expected error for done -> pending transition")
	}

	// non-existent task
	_, err = svc.UpdateStatus(9999, StatusInProgress)
	if err == nil {
		t.Errorf("expected error for non-existent task")
	}
}

func TestGetTask(t *testing.T) {
	svc := NewTaskService()
	created, err := svc.CreateTask(CreateTaskRequest{
		Title:    "Find Me",
		Priority: 2,
	})
	if err != nil {
		t.Fatalf("failed to create task: %v", err)
	}

	task, err := svc.GetTask(created.ID)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}
	if task.Title != "Find Me" {
		t.Errorf("title = %q, want %q", task.Title, "Find Me")
	}

	// non-existent
	_, err = svc.GetTask(9999)
	if err == nil {
		t.Errorf("expected error for non-existent task")
	}
}

func TestListTasks(t *testing.T) {
	svc := NewTaskService()

	// empty list
	tasks := svc.ListTasks()
	if len(tasks) != 0 {
		t.Errorf("expected 0 tasks, got %d", len(tasks))
	}

	// add tasks
	for i := 1; i <= 3; i++ {
		_, err := svc.CreateTask(CreateTaskRequest{
			Title:    "Task",
			Priority: i,
		})
		if err != nil {
			t.Fatalf("failed to create task: %v", err)
		}
	}

	tasks = svc.ListTasks()
	if len(tasks) != 3 {
		t.Errorf("expected 3 tasks, got %d", len(tasks))
	}
}

func TestDeleteTask(t *testing.T) {
	svc := NewTaskService()
	task, err := svc.CreateTask(CreateTaskRequest{
		Title:    "Delete Me",
		Priority: 1,
	})
	if err != nil {
		t.Fatalf("failed to create task: %v", err)
	}

	err = svc.DeleteTask(task.ID)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// verify deleted
	_, err = svc.GetTask(task.ID)
	if err == nil {
		t.Errorf("expected error after deletion")
	}

	// delete non-existent
	err = svc.DeleteTask(9999)
	if err == nil {
		t.Errorf("expected error for non-existent task")
	}
}

func TestGetStats(t *testing.T) {
	svc := NewTaskService()

	// Create tasks with different priorities
	_, _ = svc.CreateTask(CreateTaskRequest{Title: "Low", Priority: 1})
	_, _ = svc.CreateTask(CreateTaskRequest{Title: "Med", Priority: 3})
	task3, _ := svc.CreateTask(CreateTaskRequest{Title: "High", Priority: 4})
	_, _ = svc.CreateTask(CreateTaskRequest{Title: "Critical", Priority: 5})

	// Move one to in_progress, one to done
	_, _ = svc.UpdateStatus(task3.ID, StatusInProgress)

	stats := svc.GetStats()

	if stats.Total != 4 {
		t.Errorf("total = %d, want 4", stats.Total)
	}
	if stats.Pending != 3 {
		t.Errorf("pending = %d, want 3", stats.Pending)
	}
	if stats.InProgress != 1 {
		t.Errorf("in_progress = %d, want 1", stats.InProgress)
	}
	if stats.Done != 0 {
		t.Errorf("done = %d, want 0", stats.Done)
	}
	if stats.HighPriority != 2 {
		t.Errorf("high_priority = %d, want 2", stats.HighPriority)
	}
}

func TestStatusFromString(t *testing.T) {
	tests := []struct {
		input   string
		want    TaskStatus
		wantErr bool
	}{
		{"pending", StatusPending, false},
		{"in_progress", StatusInProgress, false},
		{"done", StatusDone, false},
		{"PENDING", StatusPending, false},
		{"unknown", 0, true},
		{"", 0, true},
	}

	for _, tt := range tests {
		t.Run(tt.input, func(t *testing.T) {
			got, err := StatusFromString(tt.input)
			if tt.wantErr {
				if err == nil {
					t.Errorf("expected error for input %q", tt.input)
				}
				return
			}
			if err != nil {
				t.Errorf("unexpected error: %v", err)
				return
			}
			if got != tt.want {
				t.Errorf("StatusFromString(%q) = %d, want %d", tt.input, got, tt.want)
			}
		})
	}
}

func TestStatusToString(t *testing.T) {
	tests := []struct {
		input TaskStatus
		want  string
	}{
		{StatusPending, "pending"},
		{StatusInProgress, "in_progress"},
		{StatusDone, "done"},
		{TaskStatus(99), "unknown"},
	}

	for _, tt := range tests {
		got := StatusToString(tt.input)
		if got != tt.want {
			t.Errorf("StatusToString(%d) = %q, want %q", tt.input, got, tt.want)
		}
	}
}

func TestIDAutoIncrement(t *testing.T) {
	svc := NewTaskService()

	t1, _ := svc.CreateTask(CreateTaskRequest{Title: "First", Priority: 1})
	t2, _ := svc.CreateTask(CreateTaskRequest{Title: "Second", Priority: 1})
	t3, _ := svc.CreateTask(CreateTaskRequest{Title: "Third", Priority: 1})

	if t1.ID != 1 || t2.ID != 2 || t3.ID != 3 {
		t.Errorf("IDs = %d, %d, %d; want 1, 2, 3", t1.ID, t2.ID, t3.ID)
	}
}

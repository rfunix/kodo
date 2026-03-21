package main

import (
	"encoding/json"
	"os"
)

func SaveTasks(tasks map[uint64]*Task, filepath string) error {
	data, err := json.MarshalIndent(tasks, "", "  ")
	if err != nil {
		return err
	}

	return os.WriteFile(filepath, data, 0644)
}

func LoadTasks(filepath string) (map[uint64]*Task, error) {
	data, err := os.ReadFile(filepath)
	if err != nil {
		return nil, err
	}

	var tasks map[uint64]*Task
	if err := json.Unmarshal(data, &tasks); err != nil {
		return nil, err
	}

	return tasks, nil
}

package main

import (
	"fmt"
	"log"
	"net/http"
)

func main() {
	svc := NewTaskService()
	mux := SetupRoutes(svc)

	fmt.Println("Task Management API running on :8080")
	log.Fatal(http.ListenAndServe(":8080", mux))
}

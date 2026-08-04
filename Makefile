.PHONY: docker docker-dev docker-build docker-test docker-down run web test help

# Default target
.DEFAULT_GOAL := help

## docker: Build the web production Docker container image
docker:
	docker build -t singularity-web:latest .

## docker-dev: Start Docker Dev Mode with Compose Watch (hot-reloading)
docker-dev:
	docker compose -f docker-compose.dev.yml watch

## docker-build: Build the Docker Dev container image
docker-build:
	docker compose -f docker-compose.dev.yml build

## docker-test: Run Cargo tests inside the Docker Dev container
docker-test:
	docker compose -f docker-compose.dev.yml run --rm web-dev cargo test

## docker-down: Stop and remove Docker Dev containers
docker-down:
	docker compose -f docker-down.dev.yml down 2>/dev/null || docker compose -f docker-compose.dev.yml down

## run: Run desktop native release build
run:
	cargo run --release

## web: Run local host trunk dev server
web:
	trunk serve

## test: Run CPU physics unit and integration tests locally
test:
	cargo test

## help: Show available commands
help:
	@echo "singularity-rs Makefile"
	@echo ""
	@echo "Usage:"
	@echo "  make docker        - Build web production Docker image"
	@echo "  make docker-dev    - Start Docker Dev Mode with hot-reloading (Compose Watch)"
	@echo "  make docker-build  - Build Docker Dev container image"
	@echo "  make docker-test   - Run cargo test inside Docker container"
	@echo "  make docker-down   - Stop Docker Dev containers"
	@echo "  make run           - Run desktop native release build"
	@echo "  make web           - Run local host trunk serve"
	@echo "  make test          - Run local cargo test"

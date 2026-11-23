.PHONY: docker run web test help

# Default target
.DEFAULT_GOAL := help

## docker: Build the web production Docker container image
docker:
	docker build -t singularity-web:latest .

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
	@echo "  make run           - Run desktop native release build"
	@echo "  make web           - Run local host trunk serve"
	@echo "  make test          - Run local cargo test"

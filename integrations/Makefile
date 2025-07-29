.PHONY: up down wait-compose-ready restart logs postgres

PROJECT_ROOT := $(shell git rev-parse --show-toplevel)
DOCKER_COMPOSE_DIR := $(PROJECT_ROOT)/integrations
DOCKER_COMPOSE_CFG := $(DOCKER_COMPOSE_DIR)/docker-compose.yml

docker_compose := docker compose -f $(DOCKER_COMPOSE_CFG)
compose_down := $(docker_compose) down

postgres:
	@echo "Cleaning up PostgreSQL containers and volumes..."
	-docker rm $$(docker ps -a | grep postgres | awk '{print $$1}')
	-docker rm $$(docker ps -a | grep pgadmin | awk '{print $$1}')
	-docker volume rm docker_postgres-data
	-docker volume rm docker_pgadmin-data
	@echo "Starting fresh PostgreSQL environment..."
	docker compose -f $(DOCKER_COMPOSE_DIR)/docker-compose.postgres.yml up

check-docker:
	@command -v docker > /dev/null 2>&1 || { echo "Docker is not installed"; exit 1; }

start-celestia:
	@echo "Starting celestia docker compose..."
	$(docker_compose) up --build --force-recreate -d --quiet-pull
	echo "waiting for container 'sequencer-0' to become operational..."
	@while true; do \
		timestamp=$$(date $(DATE_FORMAT)); \
		health=$$(docker inspect --format='{{.State.Health.Status}}' $$($(docker_compose) ps -q sequencer-0)); \
		if [ "$$health" = "healthy" ]; then \
			echo "sequencer-0 is healthy"; \
			break; \
		else \
			echo "health == '$$health': Waiting for sequencer-0 to be up and running..."; \
		fi; \
		sleep 3; \
	done
	@echo "Services are ready, preparing configuration"

stop-celestia:
	$(compose_down)

.PHONY: up down wait-compose-ready restart logs postgres start-celestia stop-celestia clean check-docker

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
	@echo "waiting for container 'celestia-node-0' to become operational..."
	@while true; do \
		timestamp=$$(date '+%Y-%m-%d %H:%M:%S'); \
		health=$$(docker inspect --format='{{.State.Health.Status}}' $$($(docker_compose) ps -q celestia-node-0)); \
		if [ "$$health" = "healthy" ]; then \
			echo "[$$timestamp] celestia-node-0 is healthy"; \
			break; \
		else \
			echo "[$$timestamp] health == '$$health': Waiting for celestia-node-0 to be up and running..."; \
		fi; \
		sleep 3; \
	done
	@echo " ✔ Celestia devnet containers are ready."

stop-celestia:
	$(compose_down)

clean: ## Clean up all docker compose resources
	@echo "Cleaning up all docker compose resources..."
	$(compose_down) --volumes --remove-orphans
	@echo "Removing any dangling images..."
	-docker image prune -f
	@echo "Docker compose cleanup complete"

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

start-hyperlane-ethtest:
	@echo "Starting hyperlane ethtest docker compose..."
	mkdir -p ./hyperlane/docker-data/relayer ./hyperlane/docker-data/validator-ethtest/signatures && chmod -R 777 ./hyperlane/docker-data
	cd hyperlane && docker compose -f docker-compose.hyp-evm.yml up --build --force-recreate -d --quiet-pull
	@echo "waiting for containers to become operational (timeout: 300 seconds)..."
	@start_time=$$(date +%s); \
	timeout_seconds=300; \
	while true; do \
		timestamp=$$(date '+%Y-%m-%d %H:%M:%S'); \
		current_time=$$(date +%s); \
		elapsed=$$((current_time - start_time)); \
		validator_health=$$(docker inspect --format='{{.State.Health.Status}}' hyperlane-validator-ethtest-1 2>/dev/null || echo "not found"); \
		relayer_health=$$(docker inspect --format='{{.State.Health.Status}}' hyperlane-relayer-1 2>/dev/null || echo "not found"); \
		echo "[$$timestamp] Health check - validator: '$$validator_health', relayer: '$$relayer_health' (elapsed: $${elapsed}s)"; \
		if [ "$$validator_health" = "healthy" ] && [ "$$relayer_health" = "healthy" ]; then \
			echo "[$$timestamp] ✔ All hyperlane containers are healthy"; \
			break; \
		elif [ $$elapsed -ge $$timeout_seconds ]; then \
			echo "[$$timestamp] ✗ Timeout reached after $$timeout_seconds seconds"; \
			exit 1; \
		else \
			echo "[$$timestamp] Waiting for hyperlane containers to be up and running..."; \
		fi; \
		sleep 3; \
	done
	@echo " ✔ Hyperlane ethtest containers are ready."

stop-hyperlane-ethtest:
	cd hyperlane && docker compose -f docker-compose.hyp-evm.yml down

clean: ## Clean up all docker compose resources
	@if docker info >/dev/null 2>&1; then \
		echo "Cleaning up all docker compose resources..."; \
		$(compose_down) --volumes --remove-orphans 2>/dev/null || true; \
		docker compose -f hyperlane/docker-compose.hyp-evm.yml down --volumes --remove-orphans  2>/dev/null || true; \
		echo "Removing any dangling images..."; \
		docker image prune -f 2>/dev/null || true; \
		echo "Docker compose cleanup complete."; \
	else \
		echo "Docker is not running. Skipping integrations cleanup."; \
	fi
	rm -rf hyperlane/docker-data/*
	rm -rf hyperlane/configs/chains/ethtest/addresses.yaml
	rm -rf hyperlane/configs/deployments/*

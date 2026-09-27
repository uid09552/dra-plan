# dra-workflow developer tasks. Run `make help` for an overview.

BACKEND        := backend
FRONTEND       := frontend
CARGO          := cargo
DATABASE_URL   ?= postgres://dra:dra@localhost:5434/dra
TEST_DATABASE_URL ?= $(DATABASE_URL)

.DEFAULT_GOAL := help
.PHONY: help up down app-up app-down docker-build token db-sync keycloak-sync run-gateway db-up db-down db-reset db-wait build release run run-release migrate test lint fmt check clean ui-install ui ui-build ui-test ui-lint docs docs-serve

help: ## Show available targets
	@grep -E '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) | awk 'BEGIN {FS = ":.*?## "}; {printf "  \033[36m%-12s\033[0m %s\n", $$1, $$2}'

.env:
	@cp .env.example .env && echo "Created .env from .env.example - replace the change-me secrets" && false

up: .env db-sync ## Start the stack: PostgreSQL, Keycloak (:8180), APISIX gateway (:9080) on network drp
	docker compose up -d
	@$(MAKE) --no-print-directory keycloak-sync

down: ## Stop the stack incl. app containers (keeps data)
	docker compose --profile app down

docker-build: ## Build the container images dra-server:local and dra-ui:local
	docker compose --profile app build

app-up: .env db-sync ## Stack + backend and UI containers: UI via gateway http://localhost:9080 (login) or :8080 (dev mode!)
	docker compose --profile app up -d --build
	@$(MAKE) --no-print-directory keycloak-sync

app-down: ## Stop only the backend and UI containers
	docker compose --profile app stop backend ui

token: .env ## Print an access token for demo-user (organization/tenant demo)
	@set -a; . ./.env; set +a; curl -s "$$KEYCLOAK_PUBLIC_URL/realms/$$KEYCLOAK_REALM/protocol/openid-connect/token" \
		-d grant_type=password -d client_id="$$DRA_CLIENT_ID" -d client_secret="$$DRA_CLIENT_SECRET" \
		-d username=demo-user -d password="$$DEMO_USER_PASSWORD" -d scope=openid \
		| python3 -c "import sys, json; print(json.load(sys.stdin)['access_token'])"

db-up: .env ## Start PostgreSQL (docker compose, port 5434)
	docker compose up -d postgres

db-wait: db-up ## Wait until PostgreSQL accepts connections
	@until docker exec dra_postgres pg_isready -U dra -d dra >/dev/null 2>&1; do sleep 1; done
	@echo "PostgreSQL is ready"

db-sync: db-wait ## Apply KEYCLOAK_DB_PASSWORD from .env to the existing database (init scripts only run once)
	@set -a && . ./.env && set +a && \
	  docker exec -e KEYCLOAK_DB_PASSWORD dra_postgres sh /docker-entrypoint-initdb.d/10-keycloak.sh >/dev/null && \
	  echo "Keycloak database role in sync with .env"

keycloak-sync: ## Apply the client redirect/logout URLs from .env to the existing Keycloak realm
	@docker exec -i dra_keycloak bash < deploy/keycloak/sync-client.sh

db-down: ## Stop the stack (same as down)
	docker compose down

db-reset: ## Drop all data and recreate PostgreSQL
	docker compose down -v
	$(MAKE) db-wait

build: ## Build the backend (debug)
	cd $(BACKEND) && $(CARGO) build

release: ## Build the backend (optimized)
	cd $(BACKEND) && $(CARGO) build --release

run: db-wait ## Start the backend in dev mode (mocked auth, tenant "demo", auto-migrate)
	cd $(BACKEND) && DRA_DATABASE_URL=$(DATABASE_URL) $(CARGO) run -- serve --dev-mode

run-gateway: db-wait ## Backend in dev mode on 0.0.0.0:8090 so the APISIX container can reach it
	cd $(BACKEND) && DRA_DATABASE_URL=$(DATABASE_URL) DRA_LISTEN_ADDR=0.0.0.0:8090 $(CARGO) run -- serve --dev-mode

run-release: db-wait release ## Start the optimized backend in dev mode
	cd $(BACKEND) && DRA_DATABASE_URL=$(DATABASE_URL) ./target/release/dra-server serve --dev-mode

migrate: db-wait ## Apply database migrations
	cd $(BACKEND) && DRA_DATABASE_URL=$(DATABASE_URL) $(CARGO) run -- migrate

test: db-wait ## Run unit and integration tests (needs PostgreSQL)
	cd $(BACKEND) && DRA_TEST_DATABASE_URL=$(TEST_DATABASE_URL) $(CARGO) test

lint: ## Format check + clippy (warnings are errors)
	cd $(BACKEND) && $(CARGO) fmt --check && $(CARGO) clippy --all-targets -- -D warnings

fmt: ## Format the code
	cd $(BACKEND) && $(CARGO) fmt

ui-install: ## Install frontend dependencies
	cd $(FRONTEND) && npm ci

ui: ## Start the Angular UI on http://localhost:4200 (proxies /api and /mcp to the backend; run `make run` too)
	cd $(FRONTEND) && npx ng serve

ui-build: ## Production build of the UI
	cd $(FRONTEND) && npx ng build

ui-test: ## Frontend unit tests (Vitest)
	cd $(FRONTEND) && npx ng test --watch=false

ui-lint: ## Frontend formatting check (Prettier)
	cd $(FRONTEND) && npx prettier --check "src/**/*.{ts,html,scss}"

check: lint test ui-lint ui-test ui-build ## Everything CI runs
	npx --yes @redocly/cli lint

docs: .venv-docs ## Build the MkDocs site into site/ (strict: broken links fail)
	.venv-docs/bin/mkdocs build --strict

docs-serve: .venv-docs ## Live preview of the MkDocs site on http://127.0.0.1:8000
	.venv-docs/bin/mkdocs serve

.venv-docs: requirements-docs.txt
	python3 -m venv .venv-docs && .venv-docs/bin/pip install -q -r requirements-docs.txt && touch .venv-docs

clean: ## Remove build artifacts
	cd $(BACKEND) && $(CARGO) clean
	rm -rf $(FRONTEND)/dist site

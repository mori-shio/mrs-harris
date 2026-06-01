<p align="center">
  <h1 align="center">Mrs. Harris</h1>
  <p align="center">
    A serverless distributed job scheduler built in Rust — designed to replace Jenkins for modern cloud-native teams.
  </p>
</p>

<p align="center">
  <a href="#features">Features</a> &middot;
  <a href="#quick-start">Quick Start</a> &middot;
  <a href="#architecture">Architecture</a> &middot;
  <a href="#roadmap">Roadmap</a> &middot;
  <a href="#contributing">Contributing</a> &middot;
  <a href="#license">License</a>
</p>

---

## What is Mrs. Harris?

**Mrs. Harris** is an open-source job scheduler that runs your batch jobs, cron tasks, and multi-step pipelines on **AWS Fargate** and **AWS Lambda** — without managing any long-running worker servers.

### The Problem

Jenkins remains the de facto standard for job scheduling, but it comes with real operational pain:

- **Always-on infrastructure** — Jenkins workers sit idle most of the time, yet you pay for them 24/7.
- **Plugin sprawl** — Hundreds of plugins with varying quality, security posture, and compatibility.
- **Fragile state** — Job configs live on disk; a single node failure can take down your entire CI/CD.
- **Dated UI** — The web interface hasn't aged well, making it hard to observe and debug jobs at a glance.

### How Mrs. Harris Solves This

| Jenkins pain | Mrs. Harris approach |
|---|---|
| Always-on workers | **Serverless** — Fargate tasks and Lambda functions spin up on demand and shut down automatically. You pay only for the seconds your jobs actually run. |
| Plugin sprawl | **Single binary** — Controller and worker ship as one binary. No plugins to install or update. |
| Fragile file-based state | **MySQL-backed** — All job definitions, run history, and logs are stored in a durable relational database. |
| Dated UI | **Modern Web UI** — Dark glassmorphism design, real-time log streaming via WebSocket, and a calendar heatmap for job history. |

---

## Features

- **Single binary** — One `mrs-harris` binary contains both the controller (API + scheduler) and the worker. Subcommands select the mode.
- **Serverless workers** — First-class support for AWS Fargate and AWS Lambda as execution backends.
- **DAG execution engine** — Define multi-step pipelines with task dependencies. Mrs. Harris resolves the graph, parallelizes independent tasks, and respects ordering constraints.
- **Cron scheduling** — Standard cron expressions with automatic retry (fixed / exponential backoff) and dead-letter handling.
- **Real-time log streaming** — WebSocket-based log viewer with auto-scroll. Logs are archived to local disk or S3 after completion.
- **Notifications** — Slack and Email (SMTP) alerts on job success, failure, and dead-letter events.
- **Web UI** — Built with Askama + HTMX. Includes a dashboard, job editor, run detail view, calendar heatmap (FullCalendar.js), worker management, and settings panel.
- **Multi-replica safe** — Designed for horizontal scaling on Fargate with DB-based lease acquisition, row-level locking for run numbering, and claim-based task dispatch.
- **TOML job definitions** — Import and export job configurations as TOML files for version-controlled job-as-code workflows.

---

## Quick Start

The fastest way to try Mrs. Harris is with Docker Compose.

### Prerequisites

- Docker and Docker Compose
- (Optional) Rust toolchain if building from source

### 1. Clone and configure

```bash
git clone https://github.com/YOUR_USERNAME/mrs-harris.git
cd mrs-harris

cp config/controller-docker.toml.example config/controller-docker.toml
cp .env.example .env
```

Edit `.env` to set your MySQL passwords, then update `config/controller-docker.toml` to match.

### 2. Start the services

```bash
docker-compose up --build
```

This starts three containers:
- **mysql** — MySQL 8.0 database
- **web** — Web UI, API server, and WebSocket endpoint
- **scheduler** — Cron trigger, retry, reaper, dispatcher, and log archiver

### 3. Open the UI

Navigate to **http://localhost:8080**. A default admin account (`admin` / `admin`) is created automatically on first launch.

> **Warning** — Change the default password immediately. For production, use `mrs-harris init-admin --password <secure-password>` instead.

### Building from source

```bash
cp config/controller.toml.example config/controller.toml
# Edit config/controller.toml with your database URL and settings

cargo run --bin mrs-harris -- controller --config config/controller.toml
```

See `mrs-harris --help` for all available subcommands (`web`, `scheduler`, `controller`, `migrate`, `init-admin`, `worker`).

---

## Architecture

```text
mrs-harris/
├── Cargo.toml                          # Workspace root
├── Dockerfile                          # Multi-stage build
├── docker-compose.yml                  # Local dev environment
├── config/
│   ├── controller.toml.example         # Local config template
│   └── controller-docker.toml.example  # Docker config template
├── crates/
│   ├── common/                         # Shared types, config, error types
│   ├── controller/                     # Scheduler, API server, Web UI
│   │   ├── migrations/                 # SQL migrations (sqlx)
│   │   └── templates/                  # Askama HTML templates
│   └── worker/                         # Shell execution, log capture, callbacks
└── static/                             # CSS and JS assets
```

**Key design decisions:**

- **Askama** for server-rendered HTML — no JavaScript framework, just HTMX for interactivity.
- **sqlx** with compile-time checked queries against MySQL.
- **tokio** async runtime throughout.
- **Controller / Worker separation** — the same binary, different entry points. Workers call back to the controller API on completion.

---

## Roadmap

Mrs. Harris is in **alpha**. The core scheduling loop, DAG engine, Web UI, and Fargate/Lambda integration are functional and under active development. It is **not yet recommended for production use**.

### Planned

- [ ] **RBAC & multi-tenancy** — Role-based access control and workspace isolation
- [ ] **GitHub / GitLab integration** — Trigger jobs from webhooks, report status back to commits
- [ ] **Terraform / IaC bootstrap** — One-command infrastructure provisioning
- [ ] **Plugin system** — Extensible worker types beyond Fargate and Lambda
- [ ] **REST API stabilization** — Versioned public API with OpenAPI spec
- [ ] **Comprehensive test suite** — Integration tests with testcontainers
- [ ] **Observability** — Prometheus metrics endpoint and structured JSON logging
- [ ] **Documentation site** — Guides, tutorials, and API reference

### Completed

- [x] Cron scheduling with retry and dead-letter
- [x] DAG execution engine with parallel task dispatch
- [x] AWS Fargate and Lambda worker backends
- [x] Real-time WebSocket log streaming
- [x] Web UI with dashboard, calendar, job editor, and run detail views
- [x] Slack and Email notifications
- [x] TOML-based job import/export
- [x] Multi-replica safe scheduling (DB leases, row locking)
- [x] Log archiving (local disk and S3)

---

## Job Definition Example

Mrs. Harris uses TOML files for declarative job configuration:

```toml
[job]
name = "Daily ETL Pipeline"
description = "Extract, transform, and load data from multiple sources"
job_type = "dag"
worker_type = "fargate"

[[job.tasks]]
name = "extract_users"
worker_type = "lambda"
payload = { command = "python3", args = ["extract.py", "--type=users"] }

[[job.tasks]]
name = "extract_orders"
worker_type = "lambda"
payload = { command = "python3", args = ["extract.py", "--type=orders"] }

[[job.tasks]]
name = "transform"
depends_on = ["extract_users", "extract_orders"]
payload = { command = "spark-submit", args = ["transform.py"] }

[[job.tasks]]
name = "load"
depends_on = ["transform"]
payload = { command = "spark-submit", args = ["load.py"] }
```

Import with:

```bash
cargo run --bin mrs-harris -- import --file jobs/etl_pipeline.toml
```

---

## Development

```bash
# Run tests
cargo test --workspace

# Lint
cargo clippy --workspace -- -D warnings

# Format check
cargo fmt --check --all
```

---

## Contributing

Contributions are welcome! Mrs. Harris is in its early stages, and there are many ways to help:

- **Bug reports** — Open an issue if something doesn't work as expected.
- **Feature requests** — Suggest improvements or new capabilities.
- **Code contributions** — Pick up an open issue or propose a change via pull request.
- **Documentation** — Help improve the README, add examples, or write guides.

Please open an issue before starting large changes so we can discuss the approach.

---

## License

[MIT](LICENSE)

# Ashan NVIDIA Manager (ANM)

Unified Multi-Account Management, Real-time Health Probing, Model Benchmark Intelligence Ranking, and New API Synchronizer for NVIDIA NIM APIs.

## Architecture & Features

- **Multi-Account Unified Management**:
  - Store and manage multiple NVIDIA NIM API Keys with AES-GCM at-rest encryption.
  - Per-account priority, weight, and New API channel binding.
  - Real-time latency probing and health detection (healthy, rate-limited, unauthorized).
- **Intelligent Model Catalog & Context Profiling**:
  - Real-time catalog discovery with context window limits (1M / 256K / 128K tokens).
  - Maximum output token quotas (up to 32K tokens for reasoning trace).
  - Benchmarks tracking (SWE-Bench Verified, MMLU, Terminal-Bench 2.1).
- **New API Multi-Channel Synchronization**:
  - Automatically synchronizes each NVIDIA account to its dedicated New API channel.
  - Exposes the unified alias model (`ashan-ai-model-normal`) via transparent model mapping.
  - Exposes live official upstream model IDs directly.
  - Fail-safe channel status management (disables channel upon auth failure).
- **Automated Scheduling**:
  - Background cron scheduler supporting Interval mode and Daily fixed-time mode with IANA timezones.

## Technology Stack

- **Backend**: Rust (Axum, Tokio, Reqwest, Sqlx / SQLite, AesGcm)
- **Frontend**: React 19, TypeScript, Vite (NVIDIA Emerald Dark Theme)
- **Deployment**: Docker single container (multi-stage build), Unraid WebGUI XML template support.

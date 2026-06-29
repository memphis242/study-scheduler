# Study Scheduler

Localhost-only study planning app with a Rust API, SQLite persistence, and a React calendar UI.

## Stack

- Backend: Rust, Axum, Rusqlite, SQLite
- Frontend: Vite, React, TypeScript, Tailwind, FullCalendar
- Default API address: `http://127.0.0.1:5174`
- Default frontend address: `http://127.0.0.1:5173`

## Run Locally

Install the `studytime` command on this host:

```sh
./scripts/install-studytime
```

Then launch the app from any directory:

```sh
studytime
```

The command starts the backend and frontend, waits for both to become ready, and opens the GUI. It stores the default SQLite database at `~/.local/share/studytime/study-scheduler.db`; set `STUDYTIME_DB` to override it. Use `studytime --no-open` for a terminal-only launch.

Manual development commands are still available:

Backend:

```sh
cd backend
~/.cargo/bin/cargo run
```

Frontend:

```sh
cd frontend
npm run dev -- --host 127.0.0.1
```

The backend creates `study-scheduler.db` locally unless `STUDY_SCHEDULER_DB` is set.

## Verify

Backend tests:

```sh
cd backend
~/.cargo/bin/cargo test
```

Frontend build:

```sh
cd frontend
npm run build
```

Frontend tests:

```sh
cd frontend
npm test
```

## Development Policy

- Keep commits focused and feature-level, with each commit easy to understand and revert.
- Push each completed commit to `origin/main` after relevant checks pass.
- Do not commit `study-scheduler-handoff.md`; it is ignored intentionally.
- When a bug is discovered, add regression coverage with the fix. Prefer unit, integration, and E2E coverage when the bug crosses those layers; document any level that is not practical.

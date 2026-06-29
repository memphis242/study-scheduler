# Repository Guidelines

## Project Structure & Module Organization

This is a localhost-only study scheduling app with a Rust backend and React frontend.

- `backend/`: Axum API, SQLite persistence, scheduling logic, and backend tests.
- `backend/src/scheduler.rs`: schedule generation and priority factor behavior.
- `backend/src/db.rs`: schema, persistence, and database-focused tests.
- `backend/src/routes.rs`: HTTP API routes and route integration tests.
- `frontend/`: Vite, React, TypeScript, Tailwind, and FullCalendar UI.
- `frontend/src/*.test.ts(x)`: Vitest unit and integration coverage.
- `bin/studytime` and `scripts/install-studytime`: local CLI launcher and installer.

Do not commit `study-scheduler-handoff.md`; it is intentionally ignored.

## Build, Test, and Development Commands

- `studytime`: start backend and frontend from any directory and open the GUI.
- `studytime --no-open`: start services without opening a browser.
- `cd backend && ~/.cargo/bin/cargo run`: run only the API on `127.0.0.1:5174`.
- `cd frontend && npm run dev -- --host 127.0.0.1`: run only the UI on `127.0.0.1:5173`.
- `cd backend && ~/.cargo/bin/cargo test`: run backend unit and route tests.
- `cd frontend && npm test`: run Vitest frontend tests.
- `cd frontend && npm run lint`: run Oxlint.
- `cd frontend && npm run build`: type-check and build the frontend.

## Coding Style & Naming Conventions

Rust uses standard `cargo fmt` formatting and snake_case module/function names. Keep backend logic grouped by responsibility: models in `models.rs`, persistence in `db.rs`, routing in `routes.rs`, scheduling in `scheduler.rs`, and priority math in `priority.rs`.

Frontend code uses TypeScript, React components in PascalCase, helper functions in camelCase, and test files named after the module under test. Keep the UI always dark mode.

## Testing Guidelines

Use Rust’s built-in test framework for backend unit and integration-style route tests. Use Vitest and React Testing Library for frontend behavior. When a bug is discovered, add regression tests with the fix, ideally at unit, integration, and E2E levels when the bug crosses those boundaries.

## Commit & Pull Request Guidelines

Keep commits focused and easy to revert. Follow the existing conventional style, for example `feat: add studytime launcher`, `test: expand backend scheduler coverage`, or `docs: add frontend test command`.

Before opening a PR, include a short description, verification commands run, screenshots for UI changes, and any known gaps. Push each completed commit to `origin/main` after relevant checks pass.

## Configuration Notes

Default ports are frontend `5173` and backend `5174`. Override launcher ports with `STUDYTIME_FRONTEND_PORT` and `STUDYTIME_API_PORT`. The default launcher database is `~/.local/share/studytime/study-scheduler.db`; override with `STUDYTIME_DB`.

# AGENTS.md — working rules for humans and coding agents

`CLAUDE.md` imports this file.

## Before you start

1. Read [docs/SPEC.md](docs/SPEC.md) (what to build, rules R1–R8, milestones) and the ADRs in
   [docs/adr/](docs/adr/). Do not work against an ADR; propose a new one instead.
2. Work on a branch, open a pull request; `main` is protected by review.

## Commands — always through the Taskfile

| Command | Purpose |
|---|---|
| `task check` | Formatting, clippy, all tests, frontend build, image build. Must be green before every commit. Starts the PostgreSQL test container (`task db:test`) for the contract tests. |
| `task db:test` / `task db:test:stop` | PostgreSQL for the contract tests (`CLOCKOPUS_TEST_DATABASE_URL`); without it those tests fail on purpose |
| `task fmt` | Format backend (`cargo fmt`) and frontend (Prettier) |
| `task backend:run` / `task frontend:run` | Run natively (backend :8080 without sign-in, Angular dev server :4200) |
| `task up` | Run the image with docker compose on http://localhost:8080 |

A command that is missing goes into `Taskfile.yml`, not into ad-hoc scripts.

## Rules

- **All tests green, backend and frontend.** If your change breaks the other side, fixing it is
  part of your change.
- **New logic gets tests, bug fixes get a regression test** — and the regression test must fail on
  the old code once.
- **Both storage backends behave the same.** Rules live in the domain layer and are covered by the
  shared contract tests (`backend/src/store/contract.rs`); a new storage behaviour gets a scenario
  there, which runs against YAML and PostgreSQL.
- **Schema changes only as a new migration** in `backend/migrations/` (embedded at build time,
  applied at startup). A merged migration is never edited. Data the YAML backend reads must stay
  readable: new fields are optional with defaults.
- **SQL with binds only.** sqlx refuses dynamic SQL strings; keep it that way.
- **The wire format is tested on both sides.** The frontend builds each request body in a named
  function; backend tests use the same literal.
- **Time:** store instants; compute days, weeks and the forgotten timer in the user's time zone.
  Test midnight and daylight-saving days.
- **Security:** the backend checks every token and every ownership itself; the frontend only hides
  what the user cannot do. No secrets in the repository.
- **Dependencies:** new ones need a one-line reason in the pull request. Check default features
  against the distroless runtime image (no system libraries besides glibc and libgcc).
- **Only the scope of the task.** No drive-by refactorings.

## Docs stay in sync

| Change | Update |
|---|---|
| Behaviour, rules, API, configuration | `docs/SPEC.md` |
| Architecture decision | new ADR in `docs/adr/` (ADRs are append-only; supersede, do not rewrite) |
| Commands, setup | `README.md`, `Taskfile.yml`, this file |

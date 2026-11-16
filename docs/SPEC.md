# clockopus — Specification v1

- **Status:** draft for review
- **Date:** 2026-11-16
- **Architecture:** [ADR-0001](adr/0001-architecture.md)

## 1. Purpose and principles

clockopus helps one person understand where their working time goes. It is built for frequent
context switches.

1. **Personal.** Every user sees only their own tasks, times and tags. There is no admin view of
   other people's data. This is a design rule, not a missing feature: time tracking that others
   can inspect becomes performance monitoring.
2. **Fast.** Creating and starting a task takes one input and Enter. Switching takes one click.
3. **Forgiving.** Mistakes are expected; correcting them is easy, and corrections are visible
   afterwards.
4. **Readable data.** In the simple mode the data lives in YAML files a human can read.

## 2. Concepts

| Term | Meaning |
|---|---|
| **User** | Created on first sign-in, identified by `(issuer, sub)`. Holds settings. |
| **Task** | Something you spend time on. Title (required), tags (optional), link (optional URL), status `active` or `done`, pinned flag. Belongs to one user. |
| **Time entry** | A span of time on one task: `start`, `end` (empty while running), `source` (`timer` or `manual`), optional note, history of corrections. |
| **Tag** | A label, per user, unique by name (case-insensitive). Optional colour. |
| **Settings** | Time zone (IANA), workday end (default `17:00`), forgotten-timer threshold (default 10 h), week start (default Monday), CSV rounding. |

## 3. Rules

### 3.1 Timer

- **R1 — one running entry.** A user has at most one running time entry.
- **R2 — start switches.** Starting task B while A runs ends A's entry at the same instant and
  starts a new entry for B. No gap, no overlap.
- **R3 — stop.** Stop ends the running entry now. An optional end time in the past is allowed
  (≥ start).
- Starting a task updates its `last_used_at`. Starting a `done` task reopens it (status `active`)
  and puts it back on the board.

### 3.2 Manual entries and corrections

- **R4 — valid span.** `start < end ≤ now`. Entries in the future are rejected.
- **R5 — no overlaps.** Entries of one user never overlap, including the running entry (counted as
  `[start, now)`). A conflicting request is rejected with `409` and names the conflicting entry.
- **R6 — history.** When `start` or `end` of an existing entry is changed, the previous values are
  appended to the entry's `history` with `changed_at` and a `reason` (`edit` or
  `forgotten-timer`). Normal stopping by the timer is not a correction and leaves no history.
  Deleting an entry removes it with its history.
- Entries may cross midnight; reports split them by day in the user's time zone.

### 3.3 Forgotten timer

- **R7 — detection.** A running entry is *forgotten* if it started before today (user's time zone)
  or has run longer than the threshold (default 10 h).
- **R8 — prompt.** The app shows a banner, never changes data on its own:
  > Your timer for **Fix login bug** has been running since **yesterday 09:12** (23 h 40 min).
  > [Stop at 17:00 yesterday] [Pick end time…] [Keep running]
  - *Stop at workday end*: offered only if the workday end on the start day lies after the start.
  - *Pick end time*: date and time picker, validated by R4/R5.
  - *Keep running*: hides the banner for this entry until it is stopped.
  - Corrections made here get `reason: forgotten-timer` in the history.

### 3.4 Tasks and tags

- Marking a task `done` removes it from the board; it stays in reports and in search, and can be
  reopened. Deleting a task deletes its entries (with confirmation showing the tracked total).
- Tags are created implicitly when used. Renaming a tag renames it everywhere; deleting a tag
  removes it from all tasks.
- **Quick-access tags:** the 8 tags with the most tracked time in the last 30 days.

## 4. User interface

English UI, responsive (desktop first, usable on a phone), light and dark theme from the system
setting, fully usable with the keyboard.

### 4.1 Board (start page)

- **Quick-add field** at the top (focus with `n` or `Ctrl+K`). Input like
  `Review payment PR #review #team-a`: words starting with `#` become tags, with auto-completion
  from existing tags. `Enter` creates and starts the task, `Alt+Enter` only creates it.
- **Quick-access tag bar** below it: clicking a tag inserts it into the quick-add field.
- **Cards**, ordered: running task first, then pinned tasks, then by `last_used_at`. A card shows
  title, tags, link icon, time tracked today, and the buttons **Start**/**Stop** and **Edit**.
  Menu: pin, mark done, delete. The running card is highlighted and shows elapsed time live.
- **Filter** by text or tag above the cards; toggle to show done tasks.
- **Browser tab title** while a timer runs: `▶ 0:42 Fix login bug`.
- **Forgotten-timer banner** (R8) above everything else.

### 4.2 Edit dialog (per task)

Title, tags, link; list of the task's time entries (newest first) with inline editing of start,
end and note; **Add time** for a manual entry; history of an entry shown on demand.

### 4.3 Reports

Range selector: **Day**, **Week**, **Month**, custom range; previous/next; “today”.

- **Day:** timeline from first to last entry; blocks per entry, coloured by task; dragging a block
  edge corrects the entry (R4–R6). Totals per task and hours per tag beside it.
- **Week:** grid tasks × days with daily and weekly totals; hours per tag.
- **Month:** calendar with the total per day; click a day to open the day view; totals per task
  and hours per tag for the month.
- **Hours per tag:** every tag of a task receives the full time of that task's entries. Because a
  task can have several tags, the sum over tags can exceed the total; the view says so in one line.
  Time on untagged tasks is shown as “untagged”.
- Format: cards and timelines `h:mm`; report totals in hours with one decimal (`3.5 h`).

### 4.4 Settings

Time zone (pre-filled from the browser on first sign-in), workday end, forgotten-timer threshold,
week start, CSV rounding. **Export my data** (YAML archive), **Import** (into an empty account
only), **Delete my account** (all data, with confirmation).

## 5. Reports and export semantics

- All day boundaries in the user's time zone; daylight-saving days have 23 or 25 hours.
- A running entry counts until now.
- **CSV export** for a range: one row per entry and day (entries crossing midnight are split),
  columns `date, task, tags, start, end, duration_minutes, note`. Optional rounding of each row to
  5 or 15 minutes (nearest); off by default (setting `csv_rounding`, overridable per download).
  Times are local `HH:MM`; a piece that ends at midnight ends at `24:00`; tags are separated by
  spaces. Comma-separated, UTF-8, quoted where needed. Cells that start with `=`, `+`, `-`, `@`,
  tab or carriage return get a leading `'`, so a title cannot run as a spreadsheet formula.
- **Week** starts on the `week_start` setting (Monday by default); the browser computes the range
  from it and asks `GET /reports` for those days.
- **Data export** (`GET /export`): one YAML document with `version`, `exported_at`, `settings`,
  `tags` and `tasks` in the shapes of section 6.1. **Import** (`POST /import`) accepts exactly that,
  only into an empty account (no tasks, no tags), and checks it like the running app does: unique
  ids, `start < end`, at most one running entry, no overlaps. Problems are `400 import_invalid`.
- **Delete my account** removes all files or rows of the user. Using clockopus again afterwards
  starts a new, empty account (the first visit sends the time zone again).

## 6. Data

### 6.1 YAML files (backend `yaml`)

Directory per user under the data directory (`local/` in auth mode `none`). Times are written in
RFC 3339 with the offset of the user's time zone, so they read as local time and stay
unambiguous. Files are written by clockopus; edit them by hand only while it is stopped.

`tasks.yaml`:

```yaml
# Written by clockopus. Times are local with UTC offset (RFC 3339).
version: 1
tasks:
  - id: 0193a1c2-7d40-7b1e-9c3a-2f1d5e6a8b90
    title: Fix login bug
    tags: [support, customer-x]
    link: https://jira.example.com/browse/ABC-123
    status: active          # active | done
    pinned: false
    created_at: "2026-11-17T09:12:00+01:00"
    entries:
      - id: 0193a1c2-7d41-7c00-8d2e-1a2b3c4d5e6f
        start: "2026-11-17T09:12:00+01:00"
        end: "2026-11-17T10:40:00+01:00"
        source: timer       # timer | manual
      - id: 0193a1c2-7d42-7a11-9e3f-6f5e4d3c2b1a
        start: "2026-11-17T14:00:00+01:00"
        end: "2026-11-17T17:00:00+01:00"
        source: timer
        note: pairing with Anna
        history:
          - changed_at: "2026-11-18T08:03:00+01:00"
            reason: forgotten-timer
            start: "2026-11-17T14:00:00+01:00"
            end: null
```

`tags.yaml`:

```yaml
version: 1
tags:
  - name: support
    color: "#6D5BD0"
  - name: customer-x
```

`settings.yaml` holds the settings from section 2:

```yaml
version: 1
timezone: Europe/Berlin
workday_end: "17:00"
forgotten_after_hours: 10
week_start: monday     # monday | sunday
csv_rounding: 0        # 0 | 5 | 15
```

Fields added later are optional when reading; missing ones take their defaults.

A running entry the user chose to keep (R8 “Keep running”) carries `keep_running: true` until it
is stopped.

### 6.2 PostgreSQL (backend `postgres`)

Migrations in `backend/migrations/`, applied at startup. Tables (`0001_init.sql`):

| Table | Content |
|---|---|
| `users` | `key` (the user), `version` (grows with every change), the settings as columns |
| `tags` | `user_key`, `name`, `color`, `position` (order of creation) |
| `tasks` | the task fields; `tags` as `text[]` in the user's order |
| `entries` | `task_id`, `user_key`, `start_at`, `end_at`, `source`, `note`, `keep_running`, `history` (`jsonb`, epoch seconds) |

Database-enforced invariants, as a backstop to the domain code:

- R1: partial unique index `entries_one_running` on `entries(user_key) WHERE end_at IS NULL`.
- R4: `CHECK (end_at IS NULL OR end_at > start_at)`.
- R5: exclusion constraint `entries_no_overlap` `EXCLUDE USING gist (user_key WITH =, tstzrange(start_at, coalesce(end_at, 'infinity')) WITH &&)`,
  deferred to commit (needs `btree_gist`, a trusted extension the database owner may create).
- Tag names unique per user on `lower(name)`.

How the backend works: a write locks the user's row (`SELECT … FOR UPDATE`), applies the
operation to the user's data and writes only what changed; reads use a per-process cache while
`users.version` is unchanged. This keeps several replicas consistent. Both backends run the same
contract tests (`backend/src/store/contract.rs`).

## 7. API (overview)

JSON over HTTPS under `/api/v1`, bearer token in auth mode `oidc`. Times in RFC 3339, durations
in whole seconds. Errors: `{"error": "<stable code>", "message": "<for people>"}` with `400`,
`401`, `404` or `500`; an overlap (R5) is `409` with `"error": "overlap"` and the other entry's
`entry_id`.

| Method and path | Purpose |
|---|---|
| `GET /config` | Public runtime config for the frontend (auth mode, issuer, client ID) |
| `GET /me`, `DELETE /me` | Current user; delete all data of the user (`204`) |
| `GET/PUT /me/settings` | `timezone`, `workday_end` (`HH:MM`), `forgotten_after_hours` (1–24), `week_start` (`monday`/`sunday`), `csv_rounding` (0/5/15); `PUT` changes only the given fields. The frontend sends the browser's time zone on the first visit. |
| `GET /tasks?status=active\|done\|all`, `POST /tasks` | List in board order with `today_seconds`, `total_seconds`, `running_since`; create (optionally `start: true`). Filtering by text or tag happens in the browser. |
| `PATCH /tasks/{id}`, `DELETE /tasks/{id}` | Edit (only the given fields; `link: ""` removes the link), delete |
| `POST /tasks/{id}/start`, `POST /timer/stop` | Start (R2), stop (optional `end`; with `"reason": "forgotten-timer"` the R8 correction, which needs `end` and is recorded in the history); stopping when nothing runs is not an error |
| `GET /timer` | `{"running": …, "forgotten": null \| {task_title, start, running_seconds, suggested_end}}` (R7/R8) |
| `POST /timer/keep-running` | R8 “Keep running” |
| `GET/POST /tasks/{id}/entries`, `PATCH/DELETE /entries/{id}` | Time entries of a task (newest first) with `history`; add (R4/R5); change `start`/`end`/`note` (R6; the end of a running entry is set by stopping it); delete |
| `GET /entries?from=YYYY-MM-DD&to=YYYY-MM-DD` | Entries touching these local days (at most 366), for the day view |
| `GET /tags`, `PATCH /tags/{name}`, `DELETE /tags/{name}` | Tags with usage; rename; delete |
| `GET /reports?from=&to=` | `total_seconds`, `days` (every day of the range with its tasks), `by_task`, `by_tag` (variant A, `tag: null` = untagged); dates in the user's time zone, at most 366 days |
| `GET /export.csv?from=&to=&round=` | CSV download (section 5); `round` defaults to the setting |
| `GET /export`, `POST /import` | YAML download of all data; import of such a file into an empty account (`204`; `400 account_not_empty` / `import_invalid`) |
| `GET /healthz`, `GET /readyz` | Probes (outside `/api/v1`) |

## 8. Configuration

| Variable | Default | Meaning |
|---|---|---|
| `CLOCKOPUS_LISTEN` | `0.0.0.0:8080` | Listen address (IPv4 by default; set `[::]:8080` for IPv6) |
| `CLOCKOPUS_STORAGE` | `yaml` | `yaml` or `postgres` |
| `CLOCKOPUS_DATA_DIR` | `/data` | Directory for the YAML backend |
| `DATABASE_URL` | – | PostgreSQL connection string |
| `CLOCKOPUS_AUTH` | – (required) | `oidc` or `none` |
| `CLOCKOPUS_OIDC_ISSUER` | – | Issuer URL |
| `CLOCKOPUS_OIDC_CLIENT_ID` | – | Public client ID of the frontend |
| `CLOCKOPUS_OIDC_AUDIENCE` | client ID | Accepted `aud` (or `azp`) |
| `CLOCKOPUS_PUBLIC_URL` | – | External URL (redirects, links) |

## 9. Delivery

- **Repository** `github.com/ChiliChonka/clockopus`, public, Apache-2.0, English. Layout:
  `backend/` (Rust), `frontend/` (Angular), `charts/clockopus/`, `docs/`; `compose.yaml`,
  `Dockerfile` and `Taskfile.yml` at the root.
- **Image** `ghcr.io/chilichonka/clockopus:<version>`, multi-stage build, non-root, read-only
  root filesystem; `linux/amd64` and `linux/arm64`.
- **Chart** `oci://ghcr.io/chilichonka/charts/clockopus`: Deployment, Service, optional
  HTTPRoute or Ingress, optional CloudNativePG cluster with backup, optional NetworkPolicy, PVC for
  the YAML backend. Refuses `storage: yaml` with more than one replica.
- **docker compose:** `compose.yaml` with YAML backend and auth `none` (volume `./data`);
  `docker compose -f compose.yaml -f compose.postgres.yaml up` switches to PostgreSQL.
- **Probes:** `/healthz` (process alive) and `/readyz` (storage answers; `503` while the
  database is unreachable, so the pod leaves the load balancer without being restarted).
- **CI (GitHub Actions):** format, clippy, backend tests (both backends, PostgreSQL service),
  frontend tests, chart lint and render tests on every push; on tag `v*` image and chart are
  built, signed and pushed.

## 10. Quality

- Shared contract tests for both stores (R1–R7, history, reports across midnight and DST days).
- Frontend unit tests per component; one end-to-end smoke test against the compose setup.
- The wire format of every request the frontend sends is a named function and used verbatim in
  backend tests.

## 11. Milestones

| # | Content | Done when |
|---|---|---|
| M1 | Skeleton: repo, CI, Axum + Angular, image, compose (`none` + `yaml`) | `docker compose up` shows the empty board |
| M2 | Tasks, timer (R1–R3), quick add, tags, quick-access bar | Create, start, switch, stop in the browser |
| M3 | Edit dialog, manual entries, history (R4–R6), forgotten timer (R7/R8), day timeline | Correct yesterday's forgotten timer |
| M4 | Week, month, tag hours, CSV, YAML export/import | Reports match hand-computed fixtures |
| M5 | PostgreSQL backend, OIDC | Contract tests green on both backends; sign-in with Keycloak |
| M6 | Helm chart, release pipeline, rollout in the home cluster | `clockopus.contagged.info` usable with the family account |

## 12. Rollout in the home cluster (M6, outside this repository)

- Values in `helm-values/clockopus/`: storage `postgres` with CloudNativePG (2 instances, backup
  to Garage like n8n), HTTPRoute on the internal gateway `eg-intern`, host
  `clockopus.contagged.info`, local DNS entry in Pi-hole.
- Keycloak realm `family` (`https://auth.contagged.info/realms/family`): new public client
  `clockopus`, Standard Flow only, PKCE `S256`, redirect `https://clockopus.contagged.info/*`,
  web origin `https://clockopus.contagged.info`. The realm already exists, so the client is
  created via the admin API and then added to `keycloak-family/manifests/realm-import.yaml`.

## 13. Out of scope (v1)

Team or manager views, shared tags, projects or clients, billing, approvals, email reminders,
idle detection, native apps, API tokens for CLI or Stream Deck (candidate for v2), translations
(the UI is English; i18n can come later).

## 14. Decisions taken during review (2026-11-16)

1. **UI language:** English only in v1.
2. **Starting a done task** reopens it instead of refusing (section 3.1).

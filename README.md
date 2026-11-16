<p align="center">
  <img src="docs/assets/logo.svg" alt="clockopus logo: an octopus holding a running stopwatch and a paused clock" width="180">
</p>

<h1 align="center">clockopus</h1>

<p align="center"><b>A fast personal time tracker for people who switch context all day.</b></p>

---

> **Status:** design phase. The specification and the first architecture decision are up for
> review; there is no code yet. See [docs/SPEC.md](docs/SPEC.md) and
> [docs/adr/0001-architecture.md](docs/adr/0001-architecture.md).

## The idea

Developers, operators and application owners rarely work on one thing for long. A review here, a
support question there, back to the feature, then a call. At the end of the day the question
“where did my time go?” has no good answer, and most time trackers make it worse: they want
projects, clients and billing codes before you can press start.

clockopus is the opposite. It is a **personal** tool for your own time management:

- **One click to switch.** Every task is a card with a start button. Starting a task pauses the
  one that was running, because you only ever do one thing at a time.
- **Seconds to create a task.** Type `Review payment PR #review #team-a`, press Enter, and the
  task exists and runs. Tags are optional.
- **Forgiving.** Forgot to press stop yesterday? clockopus tells you the next morning and offers
  a one-click correction. Add or fix times by hand whenever you like; every correction is kept in
  a short history.
- **Honest answers.** See per day, week or month which tasks took your time, and how many hours
  went into each tag.
- **Yours only.** Nobody else sees your times: no team view, no manager dashboard. You can export
  everything and take it with you.

## What it covers

| Area | What you get |
|---|---|
| Tasks | Cards sorted by last use, pin favourites, mark done (hidden from the board, kept in reports), optional link (e.g. a ticket) |
| Timer | Start/stop per card, exactly one running timer, running time in the browser tab title |
| Manual times | Add, edit and delete time entries per task; overlaps are rejected; every change keeps the previous value |
| Forgotten timer | Detected the next day or after a configurable number of hours; correct it with one click |
| Tags | Per user, quick-access bar with your most used tags, auto-completion when creating tasks |
| Reports | Day timeline (drag to correct), week grid, month calendar; totals per task and hours per tag |
| Export | CSV per time range (optional rounding), full data export/import as YAML |
| Sign-in | Any OpenID Connect provider; the first login creates the account, no approval needed |

**Not covered, on purpose:** team or manager views, billing and invoices, projects or clients as
an extra layer (tags do that), approvals, reminders by email, idle detection.

## How it runs

clockopus is a single container: a Rust (Axum) backend that also serves the Angular frontend.

| | Try it out | Real installation |
|---|---|---|
| Install | `docker compose up` | Helm chart from `oci://ghcr.io/chilichonka/charts/clockopus` |
| Sign-in | none (one built-in local user) | OpenID Connect (e.g. Keycloak) |
| Storage | YAML files you can read yourself: `tasks.yaml`, `tags.yaml` | PostgreSQL |

The YAML files are meant to be read: times are written as local time with offset, tags by name.
You can analyse them with any script you like.

## License

[Apache License 2.0](LICENSE)

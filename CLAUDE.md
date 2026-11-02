@AGENTS.md

## Claude Code specifics

The project rules are in `AGENTS.md` (imported above), shared with any other
coding agent. This section covers only what is specific to Claude Code.

**Project skills** (in `.claude/skills`):

| Skill | Use it to… |
| :- | :- |
| `/next-task [task-id]` | Pick the next unblocked roadmap task, or the one named, and carry it through the definition of done |
| `/add-sql-feature <feature>` | Add a statement, clause, type, operator or function end to end: parser, binder, planner, executor, SLT tests, SQL reference |
| `/change-on-disk-format <structure> <change>` | Change any on-disk structure safely: version bump, compatibility, fixtures, crash coverage |
| `/evaluate-dependency <crate> <need>` | Run the build-vs-buy evaluation before anyone proposes a third-party crate |
| `/record-decision <title>` | Append an ADR to `docs/decisions.md` |

**Path-scoped rules** in `.claude/rules` load automatically when you open
matching files:

| Rule file | Applies to |
| :- | :- |
| `dependencies.md` | Cargo manifests, `Cargo.lock`, `.cargo/`, `xtask`, the toolchain file |
| `storage-and-durability.md` | `os`, `storage`, the encodings in `types`, format fixtures |
| `untrusted-input.md` | Every parser and decoder: `sql`, `storage`, `types`, `wire`, `core`'s CSV and varints, the server, fuzz corpora |
| `sql-semantics.md` | `sql`, `plan`, `exec`, `types`, SLT files, `docs/sql-reference.md` |
| `os-and-unsafe.md` | `os` |

**Permissions:**

- `.claude/settings.json` pre-approves routine `cargo` commands and
  read-only Git commands.
- It denies Git operations that publish or finalize changes, plus
  `cargo publish` and `cargo login`.
- It asks before `cargo add`, `cargo remove`, `cargo update` and
  `cargo install`.
- These are guardrails, not a security boundary. The rules in `AGENTS.md`
  still apply when a command slips past a pattern.

**Working style:**

- Use the task list for any roadmap task with more than two steps. Make the
  last item "Verify and report", and run the task's verification commands
  for it.
- Most tasks touch one or two crates. If a change spreads across more than
  three crates, stop and say so: the task is probably mis-scoped.
  `/add-sql-feature` is the exception, since a SQL feature crosses `sql`,
  `plan`, `exec` and the tests by design.
- When an acceptance criterion needs an OS this session can't run (Claude
  Code sessions are usually Linux), implement it and say plainly which
  checks still need that OS.
- When a task depends on a `Proposed` ADR, ask the user to accept, change
  or reject it before writing code that relies on it.
- Long campaigns (`cargo xtask crash` with large iteration counts, long
  fuzz runs) can run in the background; report the seed and the iteration
  count actually reached.

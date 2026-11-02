---
name: next-task
description: Pick the next unblocked task from docs/roadmap.md (or the task ID given) and carry it through the project's definition of done, leaving changes uncommitted for review.
argument-hint: "[task-id, e.g. M1.2]"
disable-model-invocation: true
---

# Work the next roadmap task

Requested task: `$ARGUMENTS` (empty means "pick the next unblocked task").

## 1. Choose the task

1. Open `docs/roadmap.md`.
2. If a task ID was given, use it. If its `Depends` tasks are not all
   checked, stop and tell the user which ones block it.
3. Otherwise, take the **first** unchecked task whose `Depends` are all
   checked.
   - Skip a task whose remaining work is marked `⏳ needs <OS>` when this
     session cannot run that OS.
   - Mention any task you skipped and why.
4. Check the ADRs the task relies on (`docs/decisions.md`). If one is
   `Proposed`, show the user its decision in two or three lines and ask
   them to accept, change or reject it before you write code that depends
   on it. Record the answer by changing only its `Status` line (and, if
   they changed it, by a new ADR via `/record-decision`).
5. Tell the user in one or two sentences which task you're starting and how
   you plan to approach it. Then create a task list. Its last item is
   "Verify and report".

## 2. Load context, and only the context you need

- `AGENTS.md` is already loaded. Re-read its "Non-negotiable rules" if
  unsure.
- Read the docs that `AGENTS.md` → "Read before working" lists for the
  crates the task touches, especially the relevant subsystem spec section.
- Read the existing code in those crates before writing new code. Follow
  established patterns and naming.
- If the spec and the existing code disagree, or the spec is ambiguous in a
  way that changes a public API, the on-disk format, SQL semantics, or the
  dependency matrix: stop, describe the conflict, propose the safest
  option, and ask.

## 3. Implement the smallest complete change

- Do exactly what the task's `Do` lines and acceptance criteria require.
  Nothing else: no drive-by refactors, renames or formatting of untouched
  code.
- **New third-party crate needed?** Stop and run `/evaluate-dependency`.
  Don't add it.
- **New SQL surface?** Follow `/add-sql-feature`.
- **On-disk structure changes?** Follow `/change-on-disk-format`.
- **`unsafe` or an OS call `std` lacks?** Stop: it needs an ADR first
  (`.claude/rules/os-and-unsafe.md`).
- **A design decision changed?** Run `/record-decision` and update the
  affected doc.
- Write the tests the acceptance criteria call for, beside the code.
- If the change spreads beyond three crates (other than a SQL feature's
  `sql` → `plan` → `exec` → tests path), pause and tell the user the task
  may be mis-scoped.

## 4. Verify honestly

- Run every command in the task's `Verify` line.
- Then run `cargo xtask ci` (once it exists; before M0.5, run
  `cargo build --workspace`, `cargo clippy --workspace --all-targets --
  -D warnings` and `cargo test --workspace`, and say that `xtask` does not
  exist yet).
- Storage or encoding changes: run `cargo xtask crash` and report the
  iteration count reached and the seed.
- Parser or decoder changes: run the fuzz target for at least 60 s.
- Fix failures by fixing the code. **Never** weaken a lint, delete or
  `#[ignore]` a test, loosen an SLT expectation, lower a campaign's
  iteration count, or raise a limit to make a check pass, unless the user
  explicitly agrees.
- Criteria that need another OS, real hardware, a PostgreSQL server, or
  manual observation:
  1. Do everything possible here.
  2. Leave the box unchecked.
  3. Add an indented `⏳ needs <OS/hardware/tool>: <exact remaining check>`
     line under the task in the roadmap.

## 5. Update the roadmap and docs

- Check the box **only** if every acceptance criterion is met and every
  verification command ran and passed in this session.
- Add brief indented `Note:` lines under the task where useful: the pinned
  toolchain version, measured numbers, seeds, conventions chosen,
  follow-ups for later tasks (`Note (from M2.3): …` under the later task).
- Update the subsystem spec where behavior or design actually changed, and
  `docs/sql-reference.md` statuses for SQL the task built.

## 6. Do not commit

Leave all changes uncommitted. Do not commit, amend, rebase, tag or push.
You may suggest a commit message in the report.

## 7. Report

End with a summary containing:

- **Task:** ID and title, and whether its box is now checked.
- **Changes:** what changed, and the files affected.
- **Validation:** each command run, with its real result (iterations and
  seeds for campaigns, seconds for fuzzing).
- **Not run:** checks that weren't run, and why. Include `⏳` items.
- **Assumptions** you made.
- **Unresolved issues and residual risks.**
- **Next:** the next unblocked task ID.

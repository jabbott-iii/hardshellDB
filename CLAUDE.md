@AGENTS.md

## Claude Code specifics

The project rules are in `AGENTS.md` (imported above), shared with any other
coding agent. This section covers only what is specific to Claude Code.

**Copies from Doctrina:** the rules and skills marked "Doctrina" below
are copies from [Doctrina](https://github.com/jabbott-iii/Doctrina), a
shared library of rules and skills, taken at commit `3afa240`.

- Edit only a rule's "Project details" section here. Change its general
  text in Doctrina, then copy the new version over.
- Never edit a copied skill here; it has no values of its own and reads
  them from the rules.
- Where a Doctrina rule and a hardshellDB rule differ, the hardshellDB
  rule wins, and `AGENTS.md` wins over both.

**Project skills** (in `.claude/skills`):

| Skill | Use it to… |
| :- | :- |
| `/next-task [task-id]` (Doctrina) | Pick the next unblocked roadmap task, or the one named, and carry it through the definition of done |
| `/add-sql-feature <feature>` | Add a statement, clause, type, operator or function end to end: parser, binder, planner, executor, SLT tests, SQL reference |
| `/change-on-disk-format <structure> <change>` | Change any on-disk structure safely: version bump, compatibility, fixtures, crash coverage |
| `/evaluate-dependency <crate> <need>` | Run the build-vs-buy evaluation before anyone proposes a third-party crate |
| `/record-decision <title>` | Append an ADR to `docs/decisions.md` |
| `/fix-doc-drift [scope]` (Doctrina) | Find statements in the docs that no longer match the code, the tools, or the records, and correct them |
| `/draw-diagrams [file or view]` (Doctrina) | Draw or rebuild the architecture diagrams as Mermaid in `diagram/` |
| `/phase-gates [plan \| phase ID]` (Doctrina) | Group roadmap tasks into phases that each end in a gate, or evaluate a gate |
| `/sweep-code [scope] [bugs \| dead code]` (Doctrina) | Find bugs and dead code, prove each finding, and fix the ones you approve |
| `/security-sweep [scope]` (Doctrina) | Find vulnerabilities, prove each one locally, and patch the ones you approve |
| `/simplify-code [scope]` (Doctrina) | Simplify the code you approve without giving up any functionality |
| `/set-up-github [pieces]` (Doctrina) | Write or update the files GitHub reads: CodeQL, Dependabot, `SECURITY.md`, templates, `CODEOWNERS`, `.gitignore` |
| `/build-language-packs [languages] [scope]` (Doctrina) | Translate user-facing text into nine languages as draft language packs |
| `/build-course <description>` (Doctrina) | Build a course; it stops here, because hardshellDB hasn't adopted Doctrina's `teaching-materials.md` |

**Rules** in `.claude/rules` load on their own. A rule without `paths`
front matter loads in every session; the others load when you open a file
that matches:

| Rule file | Loads for | Covers |
| :- | :- | :- |
| `writing.md` (Doctrina) | Every session | All prose: docs, comments, messages, commit messages |
| `commits.md` (Doctrina) | Every session | Commit messages, committing only on request, and how to split work into commits |
| `dependencies.md` (Doctrina) | Every session | Everything the project uses but didn't write, from crates to CI actions |
| `testing.md` (Doctrina) | Every session | Never weakening a test, coverage, strict tests, fuzzing, CI |
| `rust.md` (Doctrina) | Rust source, manifests, the toolchain and lint files | How to write Rust here |
| `roadmap-and-docs.md` (Doctrina) | `docs/`, `AGENTS.md`, `CONTRIBUTING.md`, `README.md`, diagrams | The roadmap, ADRs, specs, and status |
| `storage-and-durability.md` | `os`, `storage`, the encodings in `types`, format fixtures | Durability invariants and crash coverage |
| `untrusted-input.md` | Every parser and decoder: `sql`, `storage`, `types`, `wire`, `core`'s CSV and varints, the server, fuzz corpora | Hostile input, limits, fuzzing |
| `sql-semantics.md` | `sql`, `plan`, `exec`, `types`, SLT files, `docs/sql-reference.md` | PostgreSQL semantics and SQLSTATE codes |
| `os-and-unsafe.md` | `os` | `std` first, and `unsafe` only by ADR |

**Permissions:**

- `.claude/settings.json` pre-approves routine `cargo` commands and
  read-only Git commands.
- It denies the Git operations that publish changes or rewrite history
  (push, rebase, merge, tag, cherry-pick, `am`, `reset --hard`, and the
  filter commands), plus `cargo publish` and `cargo login`.
- It neither allows nor denies `git add` and `git commit`, so the
  permission mode decides whether Claude Code asks before each one.
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

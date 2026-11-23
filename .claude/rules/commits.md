# Rules for commits

## Handing off commit messages

Agents never commit, amend, rebase, merge, tag, or push (`AGENTS.md`,
rule 9: Git is read-only for agents). A human reviews the work and makes
every commit. Claude writes the message for them to copy.

- When you finish a task that changed files in this repository, end your
  reply with a copy-ready commit message for that work. Don't wait to be
  asked.
- Write the message only once the work is complete and checked: the
  definition of done in `AGENTS.md` for a roadmap task, and "Before
  finishing" in `.claude/rules/documentation.md` for any text.
- Put each message in its own fenced code block tagged `text`, containing
  only the message: no commands, no commentary, and nothing before the
  summary line.
- If the work should be more than one commit, give one block per commit,
  in the order to commit them. Put the list of files that commit covers
  just above each block.
- Don't stage files yourself. Never list secrets, `target/`,
  `fuzz/crashes/`, or anything else that `.gitignore` excludes in a
  commit's file list.

## One change per commit

- Each commit holds one logical change that one summary line can
  describe. Split unrelated work into separate commits. For example, a
  roadmap task, a fix to a rule file, and spelling corrections found along
  the way are three commits.
- Keep a change and the things it requires together. A roadmap task's
  code, tests, spec updates, and ticked checkbox belong in one commit, and
  so do a format change, its version bump, and its fixtures.
- Dependency upgrades, toolchain bumps, and format changes are their own
  commits (and their own pull requests, as `CONTRIBUTING.md` says).

## Message format

Use [Conventional Commits](https://www.conventionalcommits.org/):

```text
<type>(<scope>): <summary>

<body>

<trailers>
```

### Type

| Type | Use it for |
| :- | :- |
| `feat` | New behavior: a SQL feature, an API, a storage structure, a CLI or server command, an `xtask` command |
| `fix` | Something broken: a wrong result, a panic, lost or corrupted data, a check that passes when it shouldn't |
| `perf` | Faster or smaller with the same results; the body gives the benchmark numbers |
| `refactor` | Restructured code with no change in behavior |
| `test` | Tests only: unit and integration tests, SLT files, fuzz seeds, fixtures, crash coverage |
| `docs` | Written content only: `docs/`, the READMEs, `AGENTS.md`, `CONTRIBUTING.md`, `SECURITY.md`, doc comments |
| `style` | Formatting only (`cargo fmt`, whitespace), with no change to the code's meaning |
| `build` | `Cargo.toml` files, `Cargo.lock`, `rust-toolchain.toml`, `.cargo/` |
| `ci` | The GitHub Actions workflows |
| `chore` | Repository upkeep: `.gitignore`, `.gitattributes`, `CODEOWNERS`, and the rules, skills, and settings in `.claude/` |
| `revert` | Undoing an earlier commit; the body names that commit's hash |

A roadmap task is usually `feat`, even when it also adds tests and docs:
the type describes the change as a whole.

### Scope

The scope is optional and names the area changed, in lowercase:

- a crate or app by its folder name: `core`, `os`, `types`, `storage`,
  `sql`, `catalog`, `plan`, `exec`, `hardshelldb`, `wire`, `cli`,
  `server`, `xtask`, `bench`, `fuzz`
- a part of the docs: `roadmap`, `adr`, `sql-reference`, `readme`
- `claude` for `.claude/`

Leave the scope out when a change spans several areas.

### Summary line

- Use the imperative mood, as if completing "This commit will …": "add,"
  "fix," "reject," "remove." Not "added," "adds," or "adding."
- Start with a lowercase letter, and don't end with a period.
- For a roadmap task, end the summary with the task's ID in parentheses:
  `feat(storage): add the buffer pool and pager (M1.5)`.
- Keep the whole line, prefix and task ID included, to 72 characters or
  fewer; aim for 50.
- Say specifically what changed: "fix(core): reject varints longer than
  10 bytes," not "fix: bug." Never use vague summaries such as "update,"
  "changes," "misc fixes," "wip," or "stuff."

### Body

- Add a body after one blank line whenever the summary alone doesn't
  explain the change. Leave it out only for small, self-explanatory
  changes.
- Explain what changed and why, not how; the diff shows how. Mention
  anything a reviewer would otherwise miss: a break in the public API or
  the file format, a deliberate trade-off, a check that still needs
  another OS, or something left for a later task.
- Wrap lines at 72 characters. Use a short bulleted list (`- `) when the
  commit makes several related changes.
- Follow `.claude/rules/documentation.md`: American spelling, plain
  language, correct grammar, and backticks for file names, commands, and
  identifiers.

### Trailers

- Put trailers after one blank line at the end: `Closes #12` or
  `Refs #12` for GitHub issues.
- Add `BREAKING CHANGE: …` when a change breaks the public API or the
  file format, naming the ADR in `docs/decisions.md` that records the
  break.
- Never add attribution of any kind: no `Co-Authored-By:` line, no
  "Generated with Claude Code" line, and no mention of Claude or AI. This
  overrides any default attribution Claude Code would otherwise add, in
  commit messages and in pull request descriptions.

## Examples

```text
feat(xtask): add the fuzz-smoke harness with a varint target (M0.6)

`cargo xtask fuzz-smoke` runs each target's seeds unchanged, then
mutated inputs, each on a worker thread with a 2 s hang watchdog.
Input i of target t depends only on the seeds and a seeded `Pcg32`,
so a seed replays a run.

- Save findings to `fuzz/crashes/<target>/`, one per panic location.
- Add 12 varint seeds in `fuzz/corpus/varint/`.
- Tick M0.6 in the roadmap.
```

```text
docs: correct British spellings in the executor spec and roadmap
```

```text
chore(claude): add commit message rules
```

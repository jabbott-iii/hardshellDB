# hardshellDB's additions to the writing rules

`writing.md` in this folder is the writing rule for this repository, a
copy of Doctrina's shared rule with hardshellDB's values filled in. This
file adds the rules that are hardshellDB's own. Where the two differ, this
file wins. `roadmap-and-docs.md` says how the documents in `docs/` are
organized, and `rust.md` covers doc comments.

## Specs describe the design in the present tense

The specs in `docs/` (the core documents, the subsystem specs, and the
SQL reference) describe the design in the present tense, as a contract for
code that may not exist yet. This is the one exception to the rule that a
planned feature is never described in the present tense. Everywhere else,
including the README's status, doc comments, and release notes, say
plainly what exists and what doesn't, and name the roadmap task that will
add what is missing.

## Keep the exact values

Keep exact SQLSTATE codes, limits, and defaults: "`wal_autocheckpoint`
(default 16 MiB)," not "when the WAL gets large."

## Units: KiB and KB are different numbers

Use KiB, MiB, and GiB for powers of 1,024 (page sizes, cache sizes, and
memory limits), and KB, MB, and GB only for powers of 1,000. 16 MiB is
16,777,216 bytes, and 16 MB is 16,000,000.

## Code formatting

Besides what `writing.md` lists, use code formatting for SQL keywords and
statements, SQLSTATE codes, and settings such as `work_mem`.

## Headings and dashes

- Write every heading in sentence case, document titles included:
  "04 — Quality: testing, checks, CI."
- Keep the existing forms that use a spaced em dash:
  - the `NN — Title` titles of the core documents in `docs/`
  - the `ADR-NNNN — Title` headings in `docs/decisions.md`
  - the `M1.5 — area: title` task lines in `docs/roadmap.md`
  - a lone "—" in an empty table cell

## Spelling fixes leave literals alone

- When you correct a British spelling, leave string literals, test data,
  and fixtures exactly as they are, as well as the identifiers, quoted
  names, and URLs `writing.md` lists. Changing a string literal changes
  the program's output, so it belongs to a task of its own.
- Quote third-party names exactly, even when they are British. "The CRC
  catalogue" in `crates/core` is the title of Greg Cook's reference list
  of CRC algorithms, so it keeps its spelling.
- A single "l" before "-ed" and "-ing" applies only when the word's last
  syllable isn't stressed: "canceled" and "labeled," but "controlled" and
  "compelled."
- Also write "acknowledgment" and "spelled" (not "spelt").

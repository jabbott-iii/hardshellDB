---
paths:
  - "**/*.md"
  - "docs/**"
  - "**/*.rs"
---

# Rules for writing documentation

These rules apply to everything written for people to read: `docs/` (the
overview, architecture, subsystem specs, SQL reference, decision records,
and roadmap), the READMEs, `AGENTS.md`, `CLAUDE.md`, `CONTRIBUTING.md`,
`SECURITY.md`, and Rust doc comments (`///` and `//!`, which `cargo doc`
turns into the API reference). `CONTRIBUTING.md` → "Documentation" and
`AGENTS.md` → "Definition of done" say which docs a change must update;
this file says how to write them.

Write for two readers at once: a second-year computer science student new
to the engine and a senior engineer who needs every detail.

## Keep jargon down

- Use a plain phrase whenever it is just as exact.
- Use a technical term only when it carries meaning a plain phrase can't,
  or when the reader needs the term to search for it or use the tool.
- Explain each term a student might not know the first time it appears,
  in a short clause or parenthetical. Then use it freely. For example:
  "a checkpoint (the step that copies committed pages from the write-ahead
  log into the main database file)".
- Spell out acronyms on first use, as in "write-ahead log (WAL)". Universal
  ones such as SQL and API don't need it.
- Drop insider shorthand:
  - slang such as "footgun" and "yak shave"
  - a bare reference to an architecture decision record or a roadmap task,
    such as "ADR-0009" or "M8.2", that doesn't say what the record decided
    or what the task builds. Write "ADR-0009 (after a failed sync, every
    later write fails until the database is reopened; the sync is never
    retried)" or "M8.2 (the shell's interactive prompt and script mode)".

## Don't give up meaning

- Accuracy wins over simplicity. Keep the exact numbers, limits, SQLSTATE
  codes, terms, and caveats: "`wal_autocheckpoint` (default 16 MiB)", not
  "when the WAL gets large".
- If a simpler sentence would be wrong or misleading, keep the precise
  wording and add an explanation beside it.
- Don't remove details senior engineers need. Explain them so everyone
  else can follow.

## Help newer readers follow

- Explain why a design choice was made, not only what it is.
- Introduce one idea at a time, with concrete examples. An analogy goes
  next to the real explanation, never in place of it.
- Leave out "obviously" and "trivially".
- Keep paragraphs short, and write headings that make the page easy to
  skim: "Why a failed sync is never retried", not "Notes".

## In doc comments

A reader lands on one item's page, not at the top of the crate. Explain a
term once, in the module docs (`//!`) or on the item that defines it.
Elsewhere, link to that explanation with an intra-doc link such as
``[`SqlState`]`` instead of repeating it on every item.

## Use American English

- Spelling:
  - -or, not -our: color, behavior, neighbor, honor
  - -er, not -re: center, centered
  - -ize, not -ise, where British English uses -ise: initialize, serialize,
    normalize
  - one l before -ed and -ing when the word's last syllable isn't
    stressed: canceled, labeled, modeling, tunneling (but controlled and
    compelled keep two)
  - and gray, catalog, spelled (not spelt), acknowledgment, and license as
    both noun and verb
- Grammar: write complete sentences whose subjects and verbs agree. Keep
  one tense for the same kind of statement.
- Punctuation: use the serial comma (the comma before the final "and" or
  "or" in a list of three or more), as in "numbers, limits, and caveats".
  Most existing pages leave it out; add it in the sentences you write or
  change.
- Quote names exactly: code identifiers, SQL keywords, file names, the
  fields of APIs and file formats, command output, and third-party names
  keep their own spelling, even when it is British.
- Fix only the text you change. Don't sweep a whole page for spelling or
  commas as part of another task (`AGENTS.md`, rule 8: the smallest
  complete change). Mention the ones you notice in your summary instead.

## Before finishing: the two-reader check

1. Reread the draft as a second-year computer science student. Look for
   undefined terms and skipped steps.
2. Reread it as a senior engineer. Look for anything vague, oversimplified,
   or wrong.

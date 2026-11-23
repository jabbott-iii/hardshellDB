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
`SECURITY.md`, Rust doc comments (`///` and `//!`, which `cargo doc` turns
into the API reference), code comments, and commit messages.
`CONTRIBUTING.md` → "Documentation" and `AGENTS.md` → "Definition of done"
say which docs a change must update, and `.claude/rules/commits.md` covers
the rest of a commit message. This file says how to write them.

Apply these rules to text you write or change. Spelling is the one
exception: correct British spellings wherever you see them (see "Use
American English"). Don't rewrite other existing text only to make it
conform.

## Who the docs are for

Write for two readers at once: a second-year computer science student new
to the engine and a senior engineer who needs every detail. A student
should be able to follow every sentence, and a senior engineer should find
nothing vague or wrong.

Assume the reader can program and has used Git and a terminal, but doesn't
know this project or how database engines work inside (pages, B+trees,
write-ahead logging, query planning). Don't explain programming basics
such as variables, loops, or functions.

## Keep jargon down

- Use a plain phrase when it says the same thing just as exactly: "runs at
  the same time," not "is concurrent," unless the distinction matters.
- Use a technical term only when it carries meaning a plain phrase can't,
  or when the reader needs the term to search for it, read the docs, or
  use the tool.
- Explain each term a student might not know the first time it appears,
  in a short clause or parenthetical. Then use it freely. For example, "a
  checkpoint (the step that copies committed pages from the write-ahead log
  into the main database file)."
- Spell out acronyms on first use, as in "write-ahead log (WAL)." Universal
  ones such as SQL, API, CLI, OS, CPU, I/O, and URL don't need it.
- Drop insider shorthand:
  - slang such as "footgun," "yak shave," "bikeshed," "dogfood," and "LGTM"
  - internal nicknames and private abbreviations
  - a bare reference to an architecture decision record or a roadmap task,
    such as "ADR-0009" or "M8.2," that doesn't say what the record decided
    or what the task builds. Write "ADR-0009 (after a failed sync, every
    later write fails until the database is reopened; the sync is never
    retried)" or "M8.2 (the shell's interactive prompt and script mode)."

## Don't give up meaning

- Accuracy wins over simplicity. Keep the exact numbers, limits, SQLSTATE
  codes, terms, and caveats: "`wal_autocheckpoint` (default 16 MiB)," not
  "when the WAL gets large."
- If a simpler sentence would be wrong or misleading, keep the precise
  wording and add an explanation beside it.
- Don't remove details senior engineers need. Explain them so everyone
  else can follow.

## Help newer readers follow

- Explain why a design choice was made, not only what it is: say what
  problem it solves or what it trades away.
- Introduce one idea at a time, building on what is already explained.
  Give abstract ideas a concrete example. Use an analogy only if it is
  accurate, and put it next to the real explanation, never in place of it.
- Don't make readers feel behind: leave out "obviously," "of course,"
  "trivially," "as you know," and "it's well known that."
- Keep paragraphs short (about three to five sentences) and about one
  topic. Write headings that make the page easy to skim: "Why a failed sync
  is never retried," not "Notes."

## What to say

- Lead with the point. The first sentence of a page or section says what
  it covers or what the reader gets; background comes after.
- Be specific and checkable: versions, commit hashes, links to the ADR,
  pull request, or roadmap task, and numbers with units. Say what hardware,
  OS, and build profile a measurement came from.
- Keep built and planned work apart. The specs in `docs/` describe the
  design in the present tense, as a contract for code that may not exist
  yet. Text that says what works today (the README's status, doc comments,
  release notes) says plainly what exists and what doesn't, and names the
  roadmap task that will add what is missing.
- Write instructions in the imperative, one action per numbered step, with
  the expected result after the action.
- Use code formatting (backticks) for file names, paths, commands, flags,
  environment variables, settings, function and type names, SQL keywords
  and statements, SQLSTATE codes, and literal values. Don't use it for
  product names or for emphasis.
- Put code in fenced blocks with a language tag (`rust`, `sql`, `bash`,
  `text`). Leave out shell prompts (`$`), and keep command output in a
  separate block.
- Use descriptive link text, such as "the [dependency
  policy](docs/02-dependency-policy.md)," never "click here" or a bare
  "here." Link the first mention of something, not every mention.
- Use a table to compare three or more attributes across items, with a
  header row and short cells. Use lists for sequences and sets, and prose
  for reasoning.

## In doc comments

A reader lands on one item's page, not at the top of the crate. Explain a
term once, in the module docs (`//!`) or on the item that defines it.
Elsewhere, link to that explanation with an intra-doc link such as
``[`SqlState`]`` instead of repeating it on every item.

## Use American English

- Spelling:
  - -or, not -our: color, behavior, neighbor, honor
  - -er, not -re: center, centered
  - -ize and -ization, not -ise and -isation: initialize, serialize,
    normalization
  - -yze, not -yse: analyze, paralyze
  - one l before -ed and -ing when the word's last syllable isn't
    stressed: canceled, labeled, modeling, tunneling (but controlled and
    compelled keep two)
  - and gray, catalog, judgment, acknowledgment, spelled (not spelt), and
    license as both noun and verb
- Words: "toward," "among," "while," "learned," and "percent," not
  "towards," "amongst," "whilst," "learnt," or "per cent."
- Correct British spellings in existing text too. When you see one in
  prose, a doc comment, or a code comment, fix it, even in a file you
  opened for another reason. This is the one exception to `AGENTS.md`,
  rule 8 (the smallest complete change). Put these fixes in a commit of
  their own (`.claude/rules/commits.md`).
- Leave identifiers, string literals, test data and fixtures, quoted
  names, and URLs exactly as they are. Changing a string literal changes
  the program's output, so it belongs to a task of its own.
- Quote names exactly: code identifiers, SQL keywords, file names, the
  fields of APIs and file formats, command output, and third-party names
  keep their own spelling, even when it is British. "The CRC catalogue"
  in `crates/core` is the title of Greg Cook's reference list of CRC
  algorithms, so it keeps its spelling.
- Spell product names the way their owners do: PostgreSQL, SQLite, `psql`,
  Rust, Cargo, rustup, crates.io, GitHub, Linux, macOS, Windows, and Apple
  silicon.

## Choose plain words

- Watch these common errors: its/it's, their/there/they're,
  affect/effect, then/than, lose/loose, lead/led, principal/principle,
  complement/compliment, and ensure/insure.
- Write a noun as one word and the verb as two: "the setup" but "set up
  the server." The same goes for backup/back up, shutdown/shut down,
  checkout/check out, login/log in, and rollout/roll out.
- Hyphenate a compound modifier before a noun ("a zero-dependency engine,"
  "a single-file database") but not after it ("the engine has zero
  dependencies"). Don't hyphenate after an "-ly" adverb ("a fully synced
  file").
- Prefer plain words: "use" (not "utilize" or "leverage"), "to" (not "in
  order to"), "because" (not "due to the fact that"), "for example" (not
  "e.g."), "that is" (not "i.e."), and "about" (not "approximately").
- Cut filler and hedges: "basically," "simply," "just," "very," "really,"
  "actually," and "quite." Don't call something "easy" or "simple"; the
  reader decides that.
- Don't use marketing language without evidence: "blazing fast,"
  "seamless," "robust," "cutting-edge," and "revolutionary." State the
  measurement instead.
- Use inclusive, literal terms: allowlist and denylist, primary and
  replica, "placeholder" (not "dummy"), "quick check" (not "sanity check"),
  and "they" for a person whose pronouns you don't know.

## Grammar and punctuation

- Write complete sentences whose subjects and verbs agree.
- Use the active voice and the present tense: "The checkpoint copies each
  page into the main file," not "Each page will be copied into the main
  file by the checkpoint." Use the passive only when the actor is unknown
  or doesn't matter. Keep one tense for the same kind of statement.
- Keep one idea per sentence, and split sentences longer than about 25
  words.
- Use the serial comma (the comma before the final "and" or "or" in a list
  of three or more), as in "numbers, limits, and caveats." It removes
  ambiguity in long technical lists. Most existing pages leave it out; add
  it in the sentences you write or change, but don't change other
  sentences only to add it.
- Keep lists parallel: every item starts with the same part of speech.
  Items that are full sentences end with a period; fragments don't. Don't
  mix the two in one list.
- Make every pronoun's referent clear. Write "This check rejects the page,"
  not "This rejects the page," when the previous sentence holds more than
  one noun.
- Dashes:
  - Use an em dash (—) with no spaces for a break in a sentence.
  - Use an en dash (–) for ranges: "M1–M3," "2–4 MiB."
  - Use a hyphen (-) only inside compound words.
  - Keep the existing forms that use a spaced em dash: the
    `ADR-NNNN — Title` headings in `docs/decisions.md`, the
    `M1.5 — area: title` task lines in `docs/roadmap.md`, and a lone "—"
    in an empty table cell.
- In prose, periods and commas go inside closing quotation marks. When the
  exact characters matter (a command, a value, a key), use code formatting
  so the punctuation stays outside it.
- Contractions such as "it's," "don't," and "isn't" are fine. Avoid awkward
  ones such as "it'll" and "should've."
- Write headings in sentence case: "Why a failed sync is never retried."

## Numbers, dates, and units

- In prose, spell out zero through nine and use numerals from 10 up.
  Always use numerals for versions, measurements, percentages, sizes,
  counts in tables, and anything technical (page numbers, byte offsets,
  SQLSTATE codes).
- Put a space between a number and its unit: "16 MiB," "50 ms." Don't add
  a space before "%".
- Use KiB, MiB, and GiB for powers of 1,024 (page sizes, cache sizes,
  memory limits), and KB, MB, and GB only for powers of 1,000. They are
  different numbers: 16 MiB is 16,777,216 bytes, and 16 MB is 16,000,000.
- Use thousands separators in prose ("10,000 rows") but never in versions,
  years, ports, or code.
- Write versions with a "v" when they name a release ("v1.2.0"), matching
  the `v<version>` tags in `docs/05-release.md`.
- Write dates in prose as "November 7, 2026." In ADR metadata, file names,
  and data, use ISO 8601 ("2026-11-07").

## Before finishing

1. Proofread the text you changed for spelling, names, version numbers,
   and dates. Confirm every claim against its source: the code, an ADR,
   PostgreSQL's documentation, or a command you ran in this session
   (`AGENTS.md`, rule 7: honest verification).
2. If you changed doc comments, check that the API reference builds and
   that every intra-doc link resolves:

   ```bash
   RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
   ```

3. Do the two-reader check:
   1. Reread the draft as a second-year computer science student. Look
      for undefined terms and skipped steps.
   2. Reread it as a senior engineer. Look for anything vague,
      oversimplified, or wrong.

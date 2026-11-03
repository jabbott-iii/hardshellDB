# Security Policy

## Supported versions

hardshellDB is in planning and has no releases. Until the first release,
only `main` is supported. From the first release on, this table lists the
supported versions (`docs/05-release.md`).

| Version | Supported |
| :- | :- |
| `main` | :white_check_mark: |
| Releases | None yet |

## Reporting a vulnerability

Report vulnerabilities privately, through GitHub's private vulnerability
reporting:

1. On the repository's **Security** tab, select **Report a
   vulnerability**, or go to
   <https://github.com/jabbott-iii/hardshellDB/security/advisories/new>.
2. Fill in the form. Only you, the maintainer, and collaborators the
   maintainer adds can see it.

Do not report a vulnerability in a public issue, discussion, pull request
or commit message.

Include as much of this as you can:

- The commit (`git rev-parse HEAD`) or release version.
- The OS, its version, the CPU architecture and the filesystem.
- The input that triggers it: the SQL, a small database or WAL file, a CSV
  file, or a capture of wire messages, attached to the private report.
- What happens (panic, hang, memory growth, wrong result, lost data) and
  what you expected.
- The fuzz target and seed, if a fuzzer found it.

## What to expect

- An acknowledgment within 7 days.
- An assessment within 30 days: whether it is a vulnerability, its
  severity, and a plan.
- Fixes land on `main` and reach users in the next release. The advisory
  is published when the fix is available, crediting you unless you prefer
  otherwise.

## Scope

hardshellDB treats these as hostile input, and a failure to handle them
safely is a vulnerability:

- **SQL text** passed to the engine, including from an application's users.
- **Database and WAL files**, including files from other people or damaged
  ones. Opening one must not crash, hang, exhaust memory, or write outside
  the database's own files.
- **CSV input** to the CLI's import.
- **Wire-protocol messages** to `hardshelld` (from M12), before and after
  authentication.

In scope:

- A panic, hang, stack overflow, or unbounded memory or disk use caused by
  any of the inputs above.
- Lost or corrupted committed data after a process crash or power loss, on
  a supported OS and filesystem, with `durability = Full`
  (`docs/03-platforms.md`).
- A query returning another transaction's uncommitted data.
- Reading or writing files other than the database's own through SQL.
- Authentication or authorization bypass in `hardshelld` (from M12).

Out of scope:

- Storage devices or virtual machines that acknowledge flushes they did
  not perform.
- Network filesystems, cloud-synced folders and the other unsupported
  setups in `docs/03-platforms.md`.
- Data lost with `durability = Relaxed`, which trades recent commits for
  speed by design (it must still never corrupt).
- Several processes opening one file by bypassing the engine's lock.
- Denial of service by an application that legitimately runs expensive SQL
  without `statement_timeout` or `work_mem` limits.

A full threat model is written in roadmap task M11.7.

## How the code is kept safe

- No third-party crates, so no transitive supply chain
  (`docs/02-dependency-policy.md`).
- No `unsafe` code (ADR-0013), except a counting allocator in some test
  binaries, which never ships (ADR-0017).
- Every page and WAL frame is checksummed (ADR-0009).
- Every parser and decoder is fuzzed in CI, and crash campaigns run under a
  fault-injecting file system (`docs/04-quality.md`).
- CI actions are pinned by commit SHA, with read-only default permissions.

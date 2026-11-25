## Roadmap task

<!-- The task this completes, such as M1.5, or the issue it closes: "Closes #12". -->

## What changed and why

<!-- The change, the files it touches, and the problem it solves. -->

## How it was checked

<!-- Each command you ran, its real result, and the OS you ran it on. For a
     write path, the crash campaign's iteration count and seed; for a parser
     or decoder, how long its fuzz target ran. -->

- [ ] Every acceptance criterion of the task is met
- [ ] The task's `Verify` commands ran, with their results above
- [ ] `cargo --locked xtask ci` passes
- [ ] The subsystem spec, `docs/sql-reference.md`, and `docs/decisions.md` are updated where behavior, SQL, or a decision changed
- [ ] A storage change answers the storage change review checklist in `docs/04-quality.md`
- [ ] The roadmap box is ticked only if everything above holds

## Not run, and left for later

<!-- Checks you couldn't run and why (for example, one that needs another OS),
     assumptions, residual risks, and follow-up work. A break in the
     `hardshelldb` API or the file format names the ADR that records it. -->

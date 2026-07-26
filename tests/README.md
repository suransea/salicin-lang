# Test organization

The test suite is one Cargo integration-test crate, split into focused modules
under `suite/`. Keeping one crate avoids rebuilding and relinking the compiler
for every suite.

## Layers

1. **Corpus tests** validate every `.sc` file under `fixtures/pass` or
   `fixtures/fail`. Discovery is recursive and deterministic. Passing fixtures
   are checked together as one package so compiler setup is shared; failing
   fixtures remain isolated so one error cannot mask another.
2. **Focused semantic tests** run only the fixtures needed to describe one
   behavior and assert an exit code, trap, IR property, or diagnostic fragment.
3. **Project scenarios** create temporary packages only when filesystem layout,
   manifests, locks, modules, or CLI output are the behavior under test.

Do not add a focused `salic check` test merely to prove that a fixture passes or
fails: the corpus tests already provide that coverage.

## Fixtures

- `fixtures/pass/` contains sources accepted by the frontend. A focused test may
  additionally compile and run one when runtime behavior matters.
- `fixtures/fail/` contains sources rejected by the frontend. A focused test is
  needed only when the exact diagnostic is part of the language contract.
- `fixtures/test/` is reserved for the language's built-in `test(...)` runner.

Fixtures may be grouped into feature subdirectories. Pass their relative path
to `fixture`, for example `fixture("pass", "async/ready.sc")`.

Prefer one fixture that isolates one rule. Reuse an existing fixture when a new
test only adds a stronger assertion about the same program.

## Shared support

Command construction, native linking, temporary directories, corpus discovery,
and parallel checking belong in `suite/support.rs`. Feature-specific test data
and assertions stay in the corresponding suite module.

# Testing Guide

Testing is a first-class requirement. Security tooling that is not tested is
worse than no tooling, because it creates false confidence.

## Test layers

| Layer | Location | Purpose |
| ----- | -------- | ------- |
| Unit | `#[cfg(test)]` modules next to code | Individual functions and rules |
| Integration | `crates/*/tests/` | End-to-end crate behavior |
| Fixtures | `fixtures/` | Positive/negative source samples |
| Regression | `fixtures/corpus/` | Pins expected findings to prevent regressions |
| Adversarial | `fixtures/` + parser tests | Malformed, hostile, and pathological input |

## Running tests

```bash
cargo test --all
```

Python-only CI helpers (the GitHub comment and check-run scripts) have their own
standard-library test suite:

```bash
python -m unittest discover -s tests -v
```

Run a single test:

```bash
cargo test -p soroban-scan-core -- --nocapture some_test_name
```

## Rules for security-sensitive tests

- Every rule ships with at least one **positive** fixture (must trigger) and one
  **negative** fixture (must not trigger).
- False-positive tests are as important as true-positive tests.
- Parser features require malformed-input tests.
- Any future network/CI integration requires permission and error-path tests.

## Fixture conventions

Fixtures live under `fixtures/` and are grouped by purpose:

```
fixtures/
  projects/   # whole mini-projects (manifests + source)
  corpus/     # regression corpus fixtures
```

Each fixture directory should be self-describing, for example:

```
fixtures/corpus/authorization/missing-auth-positive/
  Cargo.toml
  src/lib.rs
  expected.json      # expected findings (rule ids, lines)
```

Exact formats are defined in Phase 5. Until then, fixtures are plain source
samples.

## Determinism

Tests assert that scanning the same fixture twice produces byte-identical
machine-readable output.

## Fuzzing

The parser consumes untrusted source, so it is fuzzed with
[`cargo-fuzz`](https://github.com/rust-fuzz/cargo-fuzz). The target lives in
`fuzz/` (a separate workspace, excluded from the main build) and calls
`parse_source` on arbitrary bytes decoded as lossy UTF-8; it performs no
filesystem access.

Install the toolchain and run it locally:

```sh
rustup toolchain install nightly
cargo install cargo-fuzz
cargo fuzz run parse_source
```

CI runs a bounded smoke case from the scheduled `Fuzz` workflow:

```sh
cargo fuzz run parse_source -- -runs=2000
```

Minimize any regression with `cargo fuzz tmin parse_source <input>` and add it
under `fixtures/` as a permanent regression case.
=======
## Golden report snapshots

`fixtures/snapshots/<RULE-ID>.json` and `.sarif` pin the machine-readable
output for every corpus positive fixture. The test renders each fixture through
`report::render_json` and `report::render_sarif`, then compares the bytes to the
committed snapshot. A change to a report field, ordering rule, or rule metadata
must therefore be visible in review.

Regenerate the snapshots deliberately after reviewing the diff:

```bash
UPDATE_SNAPSHOTS=1 cargo test -p soroban-scan-core --test snapshots
```

On PowerShell:

```powershell
$env:UPDATE_SNAPSHOTS = "1"
cargo test -p soroban-scan-core --test snapshots
Remove-Item Env:UPDATE_SNAPSHOTS
```

The snapshot test does not fetch anything, execute scanned code, or depend on
the local machine: the synthetic project root is `.` and file names are stable.

# Seismic Foundry Compilers

Fork of [Foundry Compilers](https://github.com/foundry-rs/compilers) (the compilation backend for [Foundry](https://github.com/foundry-rs/foundry)) with **Seismic Mercury specification** enabled. Upstream is tracked through the `main` branch.

## What This Does

Foundry Compilers provides Solidity and Vyper compilation, caching, dependency resolution, and artifact handling for the Foundry toolchain. Seismic's fork adds the **Mercury EVM version** — when the Solc version is >= 0.8.28, all EVM version normalization resolves to `Mercury`, which enables confidential storage opcodes (`CSTORE`/`CLOAD`) on Seismic's chain. The change is minimal: a constant, a new enum variant, and normalization logic.

## Build

Rust workspace with 5 crates. MSRV: **1.88**. Version: **0.19.1**.

### macOS

```bash
# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
cargo build

# Build with all features (async, svm-solc, project-util)
cargo build --all-features
```

### Linux (Ubuntu)

```bash
# Dependencies
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libssl-dev

# Install Rust (if needed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# Build
cargo build

# Build with all features
cargo build --all-features
```

## Test

```bash
# Run all unit + doc tests (default features)
cargo test

# Check for warnings (CI does this)
RUSTFLAGS="-D warnings" cargo check
```

### Integration tests (require `full` + `project-util` features and network access for svm)

```bash
cargo test --all-features
```

### Test summary

- **30** unit tests in `foundry-compilers`
- **49** unit tests in `foundry-compilers-artifacts-solc`
- **2** unit tests in `foundry-compilers-artifacts-vyper`
- **15** unit tests in `foundry-compilers-core`
- **56** doc tests across all crates
- Integration tests in `crates/compilers/tests/` (`project.rs`, `mocked.rs`) — require features `full`, `project-util`, `test-utils`

## Code Style

- **Formatter**: `cargo +nightly fmt --all` (uses nightly-only rustfmt features; `cargo fmt` on stable works but ignores some settings)
- **Max width**: 100 chars
- **Imports**: `imports_granularity = "Crate"` (nightly)
- **Linter**: `cargo clippy --all-features` — warnings-as-errors in CI
- **Commit convention**: [Conventional Commits](https://www.conventionalcommits.org/) — `type(scope): message`

Key clippy lints (workspace-level):

- `dbg-macro`, `uninlined-format-args`, `use-self`, `redundant-clone` = warn
- `result-large-err`, `large-enum-variant` = allow

## Project Layout

```
crates/
  compilers/              Main crate: compiler abstraction, project management
    src/
      artifact_output/    Output artifact handling (configurable, HH format)
      cache/              Compilation cache (dirty detection, invalidation)
      compile/            Compilation pipeline and output types
      compilers/          Compiler abstractions
        solc/             Solc integration (compiler, parser)
        vyper/            Vyper integration
      config.rs           Project paths and configuration
      filter.rs           Source file filtering
      flatten.rs          Source flattening
      resolver/           Import/dependency resolution (graph-based)
      report/             Compilation reporting and logging
    tests/
      project.rs          Integration tests (requires: full, project-util, test-utils)
      mocked.rs           Mocked compiler tests (requires: full, project-util)
  artifacts/
    solc/                 Solc JSON artifact bindings (contract, bytecode, AST, source maps)
    vyper/                Vyper JSON artifact bindings
    artifacts/            Meta-crate re-exporting solc + vyper artifacts
  core/                   Core utilities (path handling, version detection, source discovery)
test-data/                Test fixtures (sample projects, AST, compiler output, remappings)
benches/                  Benchmarks (compile_many, read_all)
scripts/
  changelog.sh            Git cliff changelog generation
.github/
  workflows/
    ci.yml                Upstream CI (main branch) — multi-platform, nextest, cargo-hack
    seismic.yml           Seismic CI (seismic branch) — build, warnings, test
  scripts/
    install_test_binaries.sh  Installs Geth + Solc for CI
```

## Key Seismic Modifications

Three files changed from upstream:

- **`crates/core/src/utils/mod.rs`** — `MERCURY_SOLC` constant (`Version::new(0, 8, 28)`)
- **`crates/artifacts/solc/src/lib.rs`** — `Mercury` variant in `EvmVersion` enum, set as `#[default]`; normalization returns `Mercury` for solc >= 0.8.28; serializes as `"mercury"`
- **`crates/artifacts/solc/src/lib.rs`** — test cases validating Mercury EVM version behavior

## Feature Flags

| Feature              | Description                                             |
| -------------------- | ------------------------------------------------------- |
| `default`            | Enables `rustls`                                        |
| `full`               | Enables `async` + `svm-solc`                            |
| `async`              | Adds async methods via `tokio`                          |
| `svm-solc`           | Auto-detect and manage `solc` builds via `svm`          |
| `project-util`       | Temp project utilities for testing (implies `svm-solc`) |
| `rustls` / `openssl` | TLS backend for `svm` downloads                         |

## CI

GitHub Actions (`.github/workflows/`):

- **seismic.yml** (seismic branch): `cargo fmt --check` (nightly), `cargo build`, `RUSTFLAGS="-D warnings" cargo check`, `cargo test`
- **ci.yml** (main branch): Multi-platform (ubuntu, macOS, windows) × (stable, MSRV 1.88) × (default, all-features), plus `cargo-hack` feature powerset, clippy, docs, deny

## Branches

- `seismic` — production branch (PR target)
- `main` — upstream-only tracking branch

## Troubleshooting

| Problem                                       | Fix                                                                                                                                         |
| --------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------- |
| `cargo fmt` warnings about unstable features  | Expected on stable toolchain. CI uses nightly: `cargo +nightly fmt --all`. Install with `rustup toolchain install nightly`.                 |
| Integration tests don't run with `cargo test` | They require features: `cargo test --all-features`. The `project` and `mocked` tests need `full`, `project-util`, and `test-utils`.         |
| `RUSTFLAGS="-D warnings"` fails on new code   | This is the CI standard. Fix all warnings before pushing.                                                                                   |
| Build slow on first run                       | ~30s on macOS arm64. Subsequent builds are incremental.                                                                                     |
| `svm` / solc download failures in tests       | Network-dependent. Some integration tests download solc via `svm`. Ensure network access or skip with `cargo test` (default features only). |

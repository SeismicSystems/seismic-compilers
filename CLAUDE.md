# Seismic Foundry Compilers

Fork of [Foundry Compilers](https://github.com/foundry-rs/compilers) (the compilation backend for [Foundry](https://github.com/foundry-rs/foundry)) with **Seismic Mercury specification** enabled. Upstream is tracked through the `main` branch.

## What This Does

Foundry Compilers provides Solidity and Vyper compilation, caching, dependency resolution, and artifact handling for the Foundry toolchain. Seismic's fork adds the **Mercury EVM version** — when the Solc version is >= 0.8.28, all EVM version normalization resolves to `Mercury`, which enables confidential storage opcodes (`CSTORE`/`CLOAD`) on Seismic's chain. The change is minimal: a constant, a new enum variant, and normalization logic.

## Build

Rust workspace with 5 crates. MSRV: **1.88**.

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

## Code Style

- **Formatter**: `cargo +nightly fmt --all` (nightly required — see `rustfmt.toml` for settings)
- **Linter**: `cargo clippy --all-features` — warnings-as-errors in CI
- **Commit convention**: [Conventional Commits](https://www.conventionalcommits.org/) — `type(scope): message`

Key clippy lints (workspace-level):

- `dbg-macro`, `uninlined-format-args`, `use-self`, `redundant-clone` = warn
- `result-large-err`, `large-enum-variant` = allow

## Key Seismic Modifications

Files changed from upstream:

- **`crates/artifacts/solc/src/lib.rs`** — `Mercury` variant in `EvmVersion` enum, set as `#[default]`; normalization returns `Mercury` for solc >= 0.8.28; serializes as `"mercury"`; test cases validating behavior
- **`crates/core/src/utils/mod.rs`** — `MERCURY_SOLC` constant (`Version::new(0, 8, 28)`)
- **`crates/compilers/src/cache/iface.rs`** — `.1` → `.data` field access for `FlaggedStorage`
- **`crates/compilers/src/resolver/parse.rs`** — `.1` → `.data` field access for `FlaggedStorage`
- **`crates/compilers/src/compilers/vyper/parser.rs`** — `#[allow(deprecated)]` annotation
- **`Cargo.toml`** — metadata (authors, repo, homepage, description)
- **`README.md`** — fork preamble with link to upstream and PR diff
- **`.github/workflows/seismic.yml`** — Seismic CI workflow (new file)

## Feature Flags

| Feature              | Description                          |
| -------------------- | ------------------------------------ |
| `default`            | Enables `rustls`                     |
| `full`               | `async` + `svm-solc`                 |
| `async`              | Async methods via `tokio`            |
| `svm-solc`           | Auto-manage `solc` via `svm`         |
| `project-util`       | Temp project utilities for testing   |
| `rustls` / `openssl` | TLS backend for `svm` downloads      |

## CI

GitHub Actions (`.github/workflows/`):

- **seismic.yml** (seismic branch): 4 jobs — `rustfmt` (nightly fmt check), `build` (cargo build), `warnings` (`RUSTFLAGS="-D warnings" cargo check`), `test` (cargo test). Runs on push/PR to `seismic`.
- **ci.yml**: upstream-only, runs on `main` branch — not used by Seismic.

## Branches

- `seismic` — production branch (PR target)
- `main` — upstream-only tracking branch

## Troubleshooting

| Problem                                       | Fix                                                                                                                                 |
| --------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------- |
| `cargo fmt` warnings about unstable features  | Expected on stable toolchain. CI uses nightly: `cargo +nightly fmt --all`. Install with `rustup toolchain install nightly`.         |
| Integration tests don't run with `cargo test` | They require features: `cargo test --all-features`. The `project` and `mocked` tests need `full`, `project-util`, and `test-utils`. |
| `RUSTFLAGS="-D warnings"` fails on new code   | This is the CI standard. Fix all warnings before pushing.                                                                           |

# Seismic Foundry Compilers

This repository contains Seismic's fork of Foundry Compilers.

## Main changes

Super minimal:
- Added the Seismic Mercury [Specification](https://github.com/SeismicSystems/seismic-revm?tab=readme-ov-file#mercury-specification--seismics-revm) to the EvmVersion enum.
  - This is only used by foundry so that different versions can be specified/requests, and these are serialized and passed on to [ssolc](https://github.com/SeismicSystems/seismic-solidity). Currently our seismic-solidity fork is very minimal and only supports the Mercury spec so this feels overkill, but the structure is relatively easy to maintain and it will be useful in the future.
- Added the ssolc commit version to the printed output used by foundry. Given that ssolc doesn't currently adhere and support multiple semvers, it is important for debugging to know exactly which compiler build foundry is using to compile contracts.

## Structure

The upstream repository lives [here](https://github.com/foundry-rs/compilers). This fork is up-to-date with it through commit `8c5683d`. You can see this by viewing the [main](https://github.com/SeismicSystems/seismic-compilers/tree/main) branch on this repository. You can view all of our changes vs. upstream on this [here](https://github.com/SeismicSystems/seismic-compilers/compare/main...seismic).

Seismic's forks of the [reth](https://github.com/paradigmxyz/reth) stack all have the same branch structure:
- `main` or `master`: this branch only consists of commits from the upstream repository. However it will rarely be up-to-date with upstream. The latest commit from this branch reflects how recently Seismic has merged in upstream commits to the seismic branch
- `seismic`: the default and production branch for these repositories. This includes all Seismic-specific code essential to make our network run


# Foundry Compilers

| [Docs](https://docs.rs/foundry-compilers/latest/foundry_compilers/) |

Originally part of [`ethers-rs`] as `ethers-solc`, Foundry Compilers is the compilation backend for [Foundry](https://github.com/foundry-rs/foundry).

[`ethers-rs`]'s `ethers-solc` is considered to be in maintenance mode, and any fixes to it will also be reflected on Foundry Compilers. No action is currently needed from devs, although we heavily recommend using Foundry Compilers instead of `ethers-solc`.

[`ethers-rs`]: https://github.com/gakonst/ethers-rs

[![Build Status][actions-badge]][actions-url]
[![Telegram chat][telegram-badge]][telegram-url]

[actions-badge]: https://img.shields.io/github/actions/workflow/status/foundry-rs/compilers/ci.yml?branch=main&style=for-the-badge
[actions-url]: https://github.com/foundry-rs/compilers/actions?query=branch%3Amain
[telegram-badge]: https://img.shields.io/endpoint?color=neon&style=for-the-badge&url=https%3A%2F%2Ftg.sumanjay.workers.dev%2Ffoundry_rs
[telegram-url]: https://t.me/foundry_rs

## Supported Rust Versions

<!--
When updating this, also update:
- clippy.toml
- Cargo.toml
- .github/workflows/ci.yml
-->

The current MSRV (minimum supported rust version) is 1.88.

Note that the MSRV is not increased automatically, and only as part of a minor
release.

## Contributing

Thanks for your help in improving the project! We are so happy to have you! We have
[a contributing guide](./CONTRIBUTING.md) to help you get involved in the
Foundry Compilers project.

Pull requests will not be merged unless CI passes, so please ensure that your
contribution follows the linting rules and passes clippy.

## Overview

To install, simply add `foundry-compilers` to your cargo dependencies.

```toml
[dependencies]
foundry-compilers = "0.18.3"
```

Example usage:

```rust,ignore
use foundry_compilers::{Project, ProjectPathsConfig};
use std::path::Path;

// configure the project with all its paths, solc, cache etc.
let project = Project::builder()
    .paths(ProjectPathsConfig::hardhat(Path::new(env!("CARGO_MANIFEST_DIR"))).unwrap())
    .build(Default::default())
    .unwrap();
let output = project.compile().unwrap();

// Tell Cargo that if a source file changes, to rerun this build script.
project.rerun_if_sources_changed();
```

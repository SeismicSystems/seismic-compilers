//! The output of a compiled project

// The Seismic Solidity compiler (ssolc) emits warnings when literals are
// converted to shielded types. These IDs are grouped by AST context so that
// downstream tools can selectively suppress them.

/// Constructor has shielded parameter types.
/// CREATE/CREATE2 does not encrypt calldata, so values leak in deployment tx.
/// Not suppressed in any context — always relevant.
#[cfg(test)]
const SHIELDED_CONSTRUCTOR_PARAM: u64 = 10103;

/// Shielded literal warnings: literal inside `new(...)` expression args.
/// Child contract is deployed via CREATE — literal leaks in init code.
/// Not suppressed in any context — always relevant.
#[cfg(test)]
const SHIELDED_LITERAL_NEW_EXPR_INT: u64 = 10401;
#[cfg(test)]
const SHIELDED_LITERAL_NEW_EXPR_BOOL: u64 = 10404;
#[cfg(test)]
const SHIELDED_LITERAL_NEW_EXPR_ADDRESS: u64 = 10407;
#[cfg(test)]
const SHIELDED_LITERAL_NEW_EXPR_FIXEDBYTES: u64 = 10410;
#[cfg(test)]
const SHIELDED_LITERAL_NEW_EXPR_ENUM: u64 = 10413;

/// Shielded literal warnings: literal inside external call args.
/// Literal is in caller bytecode, but calldata is encrypted by TxSeismic.
/// Safe to suppress in test/script files.
const SHIELDED_LITERAL_EXT_CALL_INT: u64 = 10402;
const SHIELDED_LITERAL_EXT_CALL_BOOL: u64 = 10405;
const SHIELDED_LITERAL_EXT_CALL_ADDRESS: u64 = 10408;
const SHIELDED_LITERAL_EXT_CALL_FIXEDBYTES: u64 = 10411;
const SHIELDED_LITERAL_EXT_CALL_ENUM: u64 = 10414;

/// Shielded literal warnings: literal in other contexts (assignments, internal calls, etc.).
/// Literal is embedded in contract bytecode.
/// Safe to suppress in test/script files where bytecode is never deployed.
const SHIELDED_LITERAL_OTHER_INT: u64 = 10403;
const SHIELDED_LITERAL_OTHER_BOOL: u64 = 10406;
const SHIELDED_LITERAL_OTHER_ADDRESS: u64 = 10409;
const SHIELDED_LITERAL_OTHER_FIXEDBYTES: u64 = 10412;
const SHIELDED_LITERAL_OTHER_ENUM: u64 = 10415;

/// Threshold above which a warning code is considered a seismic warning.
const SEISMIC_WARNING_THRESHOLD: u64 = 10000;

/// Warnings suppressed in test/script files — bytecode never deployed, calldata encrypted.
const SHIELDED_WARNINGS_SUPPRESSIBLE_IN_TESTS: &[u64] = &[
    // External call context
    SHIELDED_LITERAL_EXT_CALL_INT,
    SHIELDED_LITERAL_EXT_CALL_BOOL,
    SHIELDED_LITERAL_EXT_CALL_ADDRESS,
    SHIELDED_LITERAL_EXT_CALL_FIXEDBYTES,
    SHIELDED_LITERAL_EXT_CALL_ENUM,
    // Other context
    SHIELDED_LITERAL_OTHER_INT,
    SHIELDED_LITERAL_OTHER_BOOL,
    SHIELDED_LITERAL_OTHER_ADDRESS,
    SHIELDED_LITERAL_OTHER_FIXEDBYTES,
    SHIELDED_LITERAL_OTHER_ENUM,
];

use contracts::{VersionedContract, VersionedContracts};
use foundry_compilers_artifacts::{CompactContractBytecode, CompactContractRef, Severity};
use foundry_compilers_core::error::{SolcError, SolcIoError};
use info::ContractInfoRef;
use semver::Version;
use serde::{Deserialize, Serialize};
use sources::{VersionedSourceFile, VersionedSourceFiles};
use std::{
    collections::BTreeMap,
    fmt,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};
use yansi::Paint;

use crate::{
    buildinfo::{BuildContext, RawBuildInfo},
    compilers::{
        multi::MultiCompiler, CompilationError, Compiler, CompilerContract, CompilerOutput,
    },
    resolver::GraphEdges,
    Artifact, ArtifactId, ArtifactOutput, Artifacts, ConfigurableArtifacts,
};

pub mod contracts;
pub mod info;
pub mod sources;

/// A mapping from build_id to [BuildContext].
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Builds<L>(pub BTreeMap<String, BuildContext<L>>);

impl<L> Default for Builds<L> {
    fn default() -> Self {
        Self(Default::default())
    }
}

impl<L> Deref for Builds<L> {
    type Target = BTreeMap<String, BuildContext<L>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<L> DerefMut for Builds<L> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<L> IntoIterator for Builds<L> {
    type Item = (String, BuildContext<L>);
    type IntoIter = std::collections::btree_map::IntoIter<String, BuildContext<L>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.into_iter()
    }
}

/// Contains a mixture of already compiled/cached artifacts and the input set of sources that still
/// need to be compiled.
#[derive(Clone, Debug, Default)]
pub struct ProjectCompileOutput<
    C: Compiler = MultiCompiler,
    T: ArtifactOutput<CompilerContract = C::CompilerContract> = ConfigurableArtifacts,
> {
    /// contains the aggregated `CompilerOutput`
    pub(crate) compiler_output: AggregatedCompilerOutput<C>,
    /// all artifact files from `output` that were freshly compiled and written
    pub(crate) compiled_artifacts: Artifacts<T::Artifact>,
    /// All artifacts that were read from cache
    pub(crate) cached_artifacts: Artifacts<T::Artifact>,
    /// errors that should be omitted
    pub(crate) ignored_error_codes: Vec<u64>,
    /// paths that should be omitted
    pub(crate) ignored_file_paths: Vec<PathBuf>,
    /// set minimum level of severity that is treated as an error
    pub(crate) compiler_severity_filter: Severity,
    /// When true, show seismic warnings (code >= 10000) even in test files.
    pub(crate) seismic_warnings_in_tests: bool,
    /// When true, suppress ALL seismic warnings (code >= 10000) globally.
    pub(crate) no_seismic_warnings: bool,
    /// all build infos that were just compiled
    pub(crate) builds: Builds<C::Language>,
    /// The relationship between the source files and their imports
    pub(crate) edges: GraphEdges<C::Parser>,
}

impl<T: ArtifactOutput<CompilerContract = C::CompilerContract>, C: Compiler>
    ProjectCompileOutput<C, T>
{
    /// Returns the parser used to parse the sources.
    pub fn parser(&self) -> &C::Parser {
        self.edges.parser()
    }

    /// Returns the parser used to parse the sources.
    pub fn parser_mut(&mut self) -> &mut C::Parser {
        self.edges.parser_mut()
    }

    /// Converts all `\\` separators in _all_ paths to `/`
    #[instrument(skip_all)]
    pub fn slash_paths(&mut self) {
        self.compiler_output.slash_paths();
        self.compiled_artifacts.slash_paths();
        self.cached_artifacts.slash_paths();
    }

    /// Convenience function fo [`Self::slash_paths()`]
    pub fn with_slashed_paths(mut self) -> Self {
        self.slash_paths();
        self
    }

    /// All artifacts together with their contract file name and name `<file name>:<name>`.
    ///
    /// This returns a chained iterator of both cached and recompiled contract artifacts.
    ///
    /// Borrowed version of [`Self::into_artifacts`].
    pub fn artifact_ids(&self) -> impl Iterator<Item = (ArtifactId, &T::Artifact)> + '_ {
        let Self { cached_artifacts, compiled_artifacts, .. } = self;
        cached_artifacts.artifacts::<T>().chain(compiled_artifacts.artifacts::<T>())
    }

    /// All artifacts together with their contract file name and name `<file name>:<name>`
    ///
    /// This returns a chained iterator of both cached and recompiled contract artifacts
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::ConfigurableContractArtifact, ArtifactId, Project};
    /// use std::collections::btree_map::BTreeMap;
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let contracts: BTreeMap<ArtifactId, ConfigurableContractArtifact> =
    ///     project.compile()?.into_artifacts().collect();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_artifacts(self) -> impl Iterator<Item = (ArtifactId, T::Artifact)> {
        let Self { cached_artifacts, compiled_artifacts, .. } = self;
        cached_artifacts.into_artifacts::<T>().chain(compiled_artifacts.into_artifacts::<T>())
    }

    /// This returns a chained iterator of both cached and recompiled contract artifacts that yields
    /// the contract name and the corresponding artifact
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::ConfigurableContractArtifact, Project};
    /// use std::collections::btree_map::BTreeMap;
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let artifacts: BTreeMap<String, &ConfigurableContractArtifact> =
    ///     project.compile()?.artifacts().collect();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn artifacts(&self) -> impl Iterator<Item = (String, &T::Artifact)> {
        self.versioned_artifacts().map(|(name, (artifact, _))| (name, artifact))
    }

    /// This returns a chained iterator of both cached and recompiled contract artifacts that yields
    /// the contract name and the corresponding artifact with its version
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::ConfigurableContractArtifact, Project};
    /// use semver::Version;
    /// use std::collections::btree_map::BTreeMap;
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let artifacts: BTreeMap<String, (&ConfigurableContractArtifact, &Version)> =
    ///     project.compile()?.versioned_artifacts().collect();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn versioned_artifacts(&self) -> impl Iterator<Item = (String, (&T::Artifact, &Version))> {
        self.cached_artifacts
            .artifact_files()
            .chain(self.compiled_artifacts.artifact_files())
            .filter_map(|artifact| {
                T::contract_name(&artifact.file)
                    .map(|name| (name, (&artifact.artifact, &artifact.version)))
            })
    }

    /// All artifacts together with their contract file and name as tuple `(file, contract
    /// name, artifact)`
    ///
    /// This returns a chained iterator of both cached and recompiled contract artifacts
    ///
    /// Borrowed version of [`Self::into_artifacts_with_files`].
    ///
    /// **NOTE** the `file` will be returned as is, see also
    /// [`Self::with_stripped_file_prefixes()`].
    pub fn artifacts_with_files(
        &self,
    ) -> impl Iterator<Item = (&PathBuf, &String, &T::Artifact)> + '_ {
        let Self { cached_artifacts, compiled_artifacts, .. } = self;
        cached_artifacts.artifacts_with_files().chain(compiled_artifacts.artifacts_with_files())
    }

    /// All artifacts together with their contract file and name as tuple `(file, contract
    /// name, artifact)`
    ///
    /// This returns a chained iterator of both cached and recompiled contract artifacts
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::ConfigurableContractArtifact, Project};
    /// use std::{collections::btree_map::BTreeMap, path::PathBuf};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let contracts: Vec<(PathBuf, String, ConfigurableContractArtifact)> =
    ///     project.compile()?.into_artifacts_with_files().collect();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    ///
    /// **NOTE** the `file` will be returned as is, see also [`Self::with_stripped_file_prefixes()`]
    pub fn into_artifacts_with_files(self) -> impl Iterator<Item = (PathBuf, String, T::Artifact)> {
        let Self { cached_artifacts, compiled_artifacts, .. } = self;
        cached_artifacts
            .into_artifacts_with_files()
            .chain(compiled_artifacts.into_artifacts_with_files())
    }

    /// All artifacts together with their ID and the sources of the project.
    ///
    /// Note: this only returns the `SourceFiles` for freshly compiled contracts because, if not
    /// included in the `Artifact` itself (see
    /// [`foundry_compilers_artifacts::ConfigurableContractArtifact::source_file()`]), is only
    /// available via the solc `CompilerOutput`
    pub fn into_artifacts_with_sources(
        self,
    ) -> (BTreeMap<ArtifactId, T::Artifact>, VersionedSourceFiles) {
        let Self { cached_artifacts, compiled_artifacts, compiler_output, .. } = self;

        (
            cached_artifacts
                .into_artifacts::<T>()
                .chain(compiled_artifacts.into_artifacts::<T>())
                .collect(),
            compiler_output.sources,
        )
    }

    /// Strips the given prefix from all artifact file paths to make them relative to the given
    /// `base` argument
    ///
    /// # Examples
    ///
    /// Make all artifact files relative to the project's root directory
    /// ```no_run
    /// use foundry_compilers::Project;
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let output = project.compile()?.with_stripped_file_prefixes(project.root());
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    pub fn with_stripped_file_prefixes(mut self, base: &Path) -> Self {
        self.cached_artifacts = self.cached_artifacts.into_stripped_file_prefixes(base);
        self.compiled_artifacts = self.compiled_artifacts.into_stripped_file_prefixes(base);
        self.compiler_output.strip_prefix_all(base);
        self
    }

    /// Returns a reference to the (merged) solc compiler output.
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::contract::Contract, Project};
    /// use std::collections::btree_map::BTreeMap;
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let contracts: BTreeMap<String, Contract> =
    ///     project.compile()?.into_output().contracts_into_iter().collect();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn output(&self) -> &AggregatedCompilerOutput<C> {
        &self.compiler_output
    }

    /// Returns a mutable reference to the (merged) solc compiler output.
    pub fn output_mut(&mut self) -> &mut AggregatedCompilerOutput<C> {
        &mut self.compiler_output
    }

    /// Consumes the output and returns the (merged) solc compiler output.
    pub fn into_output(self) -> AggregatedCompilerOutput<C> {
        self.compiler_output
    }

    /// Returns whether this type has a compiler output.
    pub fn has_compiled_contracts(&self) -> bool {
        self.compiler_output.is_empty()
    }

    /// Returns whether this type does not contain compiled contracts.
    pub fn is_unchanged(&self) -> bool {
        self.compiler_output.is_unchanged()
    }

    /// Returns the set of `Artifacts` that were cached and got reused during
    /// [`crate::Project::compile()`]
    pub fn cached_artifacts(&self) -> &Artifacts<T::Artifact> {
        &self.cached_artifacts
    }

    /// Returns the set of `Artifacts` that were compiled with `solc` in
    /// [`crate::Project::compile()`]
    pub fn compiled_artifacts(&self) -> &Artifacts<T::Artifact> {
        &self.compiled_artifacts
    }

    /// Sets the compiled artifacts for this output.
    pub fn set_compiled_artifacts(&mut self, new_compiled_artifacts: Artifacts<T::Artifact>) {
        self.compiled_artifacts = new_compiled_artifacts;
    }

    /// Returns a `BTreeMap` that maps the compiler version used during
    /// [`crate::Project::compile()`] to a Vector of tuples containing the contract name and the
    /// `Contract`
    pub fn compiled_contracts_by_compiler_version(
        &self,
    ) -> BTreeMap<Version, Vec<(String, impl CompilerContract)>> {
        let mut contracts: BTreeMap<_, Vec<_>> = BTreeMap::new();
        let versioned_contracts = &self.compiler_output.contracts;
        for (_, name, contract, version) in versioned_contracts.contracts_with_files_and_version() {
            contracts
                .entry(version.to_owned())
                .or_default()
                .push((name.to_string(), contract.clone()));
        }
        contracts
    }

    /// Removes the contract with matching path and name using the `<path>:<contractname>` pattern
    /// where `path` is optional.
    ///
    /// If the `path` segment is `None`, then the first matching `Contract` is returned, see
    /// [`Self::remove_first`].
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, info::ContractInfo, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let output = project.compile()?;
    /// let info = ContractInfo::new("src/Greeter.sol:Greeter");
    /// let contract = output.find_contract(&info).unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn find_contract<'a>(&self, info: impl Into<ContractInfoRef<'a>>) -> Option<&T::Artifact> {
        let ContractInfoRef { path, name } = info.into();
        if let Some(path) = path {
            self.find(path[..].as_ref(), &name)
        } else {
            self.find_first(&name)
        }
    }

    /// Finds the artifact with matching path and name
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let output = project.compile()?;
    /// let contract = output.find("src/Greeter.sol".as_ref(), "Greeter").unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn find(&self, path: &Path, name: &str) -> Option<&T::Artifact> {
        if let artifact @ Some(_) = self.compiled_artifacts.find(path, name) {
            return artifact;
        }
        self.cached_artifacts.find(path, name)
    }

    /// Finds the first contract with the given name
    pub fn find_first(&self, name: &str) -> Option<&T::Artifact> {
        if let artifact @ Some(_) = self.compiled_artifacts.find_first(name) {
            return artifact;
        }
        self.cached_artifacts.find_first(name)
    }

    /// Finds the artifact with matching path and name
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let output = project.compile()?;
    /// let contract = output.find("src/Greeter.sol".as_ref(), "Greeter").unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn remove(&mut self, path: &Path, name: &str) -> Option<T::Artifact> {
        if let artifact @ Some(_) = self.compiled_artifacts.remove(path, name) {
            return artifact;
        }
        self.cached_artifacts.remove(path, name)
    }

    /// Removes the _first_ contract with the given name from the set
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let mut output = project.compile()?;
    /// let contract = output.remove_first("Greeter").unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn remove_first(&mut self, name: &str) -> Option<T::Artifact> {
        if let artifact @ Some(_) = self.compiled_artifacts.remove_first(name) {
            return artifact;
        }
        self.cached_artifacts.remove_first(name)
    }

    /// Removes the contract with matching path and name using the `<path>:<contractname>` pattern
    /// where `path` is optional.
    ///
    /// If the `path` segment is `None`, then the first matching `Contract` is returned, see
    /// [Self::remove_first]
    ///
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, info::ContractInfo, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let mut output = project.compile()?;
    /// let info = ContractInfo::new("src/Greeter.sol:Greeter");
    /// let contract = output.remove_contract(&info).unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn remove_contract<'a>(
        &mut self,
        info: impl Into<ContractInfoRef<'a>>,
    ) -> Option<T::Artifact> {
        let ContractInfoRef { path, name } = info.into();
        if let Some(path) = path {
            self.remove(path[..].as_ref(), &name)
        } else {
            self.remove_first(&name)
        }
    }

    /// A helper functions that extracts the underlying [`CompactContractBytecode`] from the
    /// [`foundry_compilers_artifacts::ConfigurableContractArtifact`]
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{
    ///     artifacts::contract::CompactContractBytecode, contracts::ArtifactContracts, ArtifactId,
    ///     Project,
    /// };
    /// use std::collections::btree_map::BTreeMap;
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let contracts: ArtifactContracts = project.compile()?.into_contract_bytecodes().collect();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn into_contract_bytecodes(
        self,
    ) -> impl Iterator<Item = (ArtifactId, CompactContractBytecode)> {
        self.into_artifacts()
            .map(|(artifact_id, artifact)| (artifact_id, artifact.into_contract_bytecode()))
    }

    pub fn builds(&self) -> impl Iterator<Item = (&String, &BuildContext<C::Language>)> {
        self.builds.iter()
    }

    /// Returns the source graph of the project.
    pub fn graph(&self) -> &GraphEdges<C::Parser> {
        &self.edges
    }
}

impl<C: Compiler, T: ArtifactOutput<CompilerContract = C::CompilerContract>>
    ProjectCompileOutput<C, T>
{
    /// Returns whether any errors were emitted by the compiler.
    pub fn has_compiler_errors(&self) -> bool {
        self.compiler_output.has_error(
            &self.ignored_error_codes,
            &self.ignored_file_paths,
            &self.compiler_severity_filter,
            self.seismic_warnings_in_tests,
            self.no_seismic_warnings,
        )
    }

    /// Returns whether any warnings were emitted by the compiler.
    pub fn has_compiler_warnings(&self) -> bool {
        self.compiler_output.has_warning(
            &self.ignored_error_codes,
            &self.ignored_file_paths,
            self.seismic_warnings_in_tests,
            self.no_seismic_warnings,
        )
    }

    /// Panics if any errors were emitted by the compiler.
    #[track_caller]
    pub fn succeeded(self) -> Self {
        self.assert_success();
        self
    }

    /// Panics if any errors were emitted by the compiler.
    #[track_caller]
    pub fn assert_success(&self) {
        assert!(!self.has_compiler_errors(), "\n{self}\n");
    }
}

impl<C: Compiler, T: ArtifactOutput<CompilerContract = C::CompilerContract>> fmt::Display
    for ProjectCompileOutput<C, T>
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.compiler_output.is_unchanged() {
            f.write_str("Nothing to compile")
        } else {
            self.compiler_output
                .diagnostics(
                    &self.ignored_error_codes,
                    &self.ignored_file_paths,
                    self.compiler_severity_filter,
                    self.seismic_warnings_in_tests,
                    self.no_seismic_warnings,
                )
                .fmt(f)
        }
    }
}

/// The aggregated output of (multiple) compile jobs
///
/// This is effectively a solc version aware `CompilerOutput`
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct AggregatedCompilerOutput<C: Compiler> {
    /// all errors from all `CompilerOutput`
    pub errors: Vec<C::CompilationError>,
    /// All source files combined with the solc version used to compile them
    pub sources: VersionedSourceFiles,
    /// All compiled contracts combined with the solc version used to compile them
    pub contracts: VersionedContracts<C::CompilerContract>,
    // All the `BuildInfo`s of solc invocations.
    pub build_infos: Vec<RawBuildInfo<C::Language>>,
}

impl<C: Compiler> Default for AggregatedCompilerOutput<C> {
    fn default() -> Self {
        Self {
            errors: Vec::new(),
            sources: Default::default(),
            contracts: Default::default(),
            build_infos: Default::default(),
        }
    }
}

impl<C: Compiler> AggregatedCompilerOutput<C> {
    /// Converts all `\\` separators in _all_ paths to `/`
    pub fn slash_paths(&mut self) {
        self.sources.slash_paths();
        self.contracts.slash_paths();
    }

    pub fn diagnostics<'a>(
        &'a self,
        ignored_error_codes: &'a [u64],
        ignored_file_paths: &'a [PathBuf],
        compiler_severity_filter: Severity,
        seismic_warnings_in_tests: bool,
        no_seismic_warnings: bool,
    ) -> OutputDiagnostics<'a, C> {
        OutputDiagnostics {
            compiler_output: self,
            ignored_error_codes,
            ignored_file_paths,
            compiler_severity_filter,
            seismic_warnings_in_tests,
            no_seismic_warnings,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.contracts.is_empty()
    }

    pub fn is_unchanged(&self) -> bool {
        self.contracts.is_empty() && self.errors.is_empty()
    }

    /// adds a new `CompilerOutput` to the aggregated output
    pub fn extend(
        &mut self,
        version: Version,
        build_info: RawBuildInfo<C::Language>,
        profile: &str,
        output: CompilerOutput<C::CompilationError, C::CompilerContract>,
    ) {
        let build_id = build_info.id.clone();
        self.build_infos.push(build_info);

        let CompilerOutput { errors, sources, contracts, .. } = output;
        self.errors.extend(errors);

        for (path, source_file) in sources {
            let sources = self.sources.as_mut().entry(path).or_default();
            sources.push(VersionedSourceFile {
                source_file,
                version: version.clone(),
                build_id: build_id.clone(),
                profile: profile.to_string(),
            });
        }

        for (file_name, new_contracts) in contracts {
            let contracts = self.contracts.0.entry(file_name).or_default();
            for (contract_name, contract) in new_contracts {
                let versioned = contracts.entry(contract_name).or_default();
                versioned.push(VersionedContract {
                    contract,
                    version: version.clone(),
                    build_id: build_id.clone(),
                    profile: profile.to_string(),
                });
            }
        }
    }

    /// Creates all `BuildInfo` files in the given `build_info_dir`
    ///
    /// There can be multiple `BuildInfo`, since we support multiple versions.
    ///
    /// The created files have a unique identifier as their name.
    pub fn write_build_infos(&self, build_info_dir: &Path) -> Result<(), SolcError> {
        if self.build_infos.is_empty() {
            return Ok(());
        }
        std::fs::create_dir_all(build_info_dir)
            .map_err(|err| SolcIoError::new(err, build_info_dir))?;
        for build_info in &self.build_infos {
            trace!("writing build info file {}", build_info.id);
            let file_name = format!("{}.json", build_info.id);
            let file = build_info_dir.join(file_name);
            std::fs::write(&file, &serde_json::to_string(build_info)?)
                .map_err(|err| SolcIoError::new(err, file))?;
        }
        Ok(())
    }

    /// Finds the _first_ contract with the given name
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let output = project.compile()?.into_output();
    /// let contract = output.find_first("Greeter").unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn find_first(&self, contract: &str) -> Option<CompactContractRef<'_>> {
        self.contracts.find_first(contract)
    }

    /// Removes the _first_ contract with the given name from the set
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let mut output = project.compile()?.into_output();
    /// let contract = output.remove_first("Greeter").unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn remove_first(&mut self, contract: &str) -> Option<C::CompilerContract> {
        self.contracts.remove_first(contract)
    }

    /// Removes the contract with matching path and name
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let mut output = project.compile()?.into_output();
    /// let contract = output.remove("src/Greeter.sol".as_ref(), "Greeter").unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn remove(&mut self, path: &Path, contract: &str) -> Option<C::CompilerContract> {
        self.contracts.remove(path, contract)
    }

    /// Removes the contract with matching path and name using the `<path>:<contractname>` pattern
    /// where `path` is optional.
    ///
    /// If the `path` segment is `None`, then the first matching `Contract` is returned, see
    /// [Self::remove_first]
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, info::ContractInfo, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let mut output = project.compile()?.into_output();
    /// let info = ContractInfo::new("src/Greeter.sol:Greeter");
    /// let contract = output.remove_contract(&info).unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn remove_contract<'a>(
        &mut self,
        info: impl Into<ContractInfoRef<'a>>,
    ) -> Option<C::CompilerContract> {
        let ContractInfoRef { path, name } = info.into();
        if let Some(path) = path {
            self.remove(path[..].as_ref(), &name)
        } else {
            self.remove_first(&name)
        }
    }

    /// Iterate over all contracts and their names
    pub fn contracts_iter(&self) -> impl Iterator<Item = (&String, &C::CompilerContract)> {
        self.contracts.contracts()
    }

    /// Iterate over all contracts and their names
    pub fn contracts_into_iter(self) -> impl Iterator<Item = (String, C::CompilerContract)> {
        self.contracts.into_contracts()
    }

    /// Returns an iterator over (`file`, `name`, `Contract`)
    pub fn contracts_with_files_iter(
        &self,
    ) -> impl Iterator<Item = (&PathBuf, &String, &C::CompilerContract)> {
        self.contracts.contracts_with_files()
    }

    /// Returns an iterator over (`file`, `name`, `Contract`)
    pub fn contracts_with_files_into_iter(
        self,
    ) -> impl Iterator<Item = (PathBuf, String, C::CompilerContract)> {
        self.contracts.into_contracts_with_files()
    }

    /// Returns an iterator over (`file`, `name`, `Contract`, `Version`)
    pub fn contracts_with_files_and_version_iter(
        &self,
    ) -> impl Iterator<Item = (&PathBuf, &String, &C::CompilerContract, &Version)> {
        self.contracts.contracts_with_files_and_version()
    }

    /// Returns an iterator over (`file`, `name`, `Contract`, `Version`)
    pub fn contracts_with_files_and_version_into_iter(
        self,
    ) -> impl Iterator<Item = (PathBuf, String, C::CompilerContract, Version)> {
        self.contracts.into_contracts_with_files_and_version()
    }

    /// Given the contract file's path and the contract's name, tries to return the contract's
    /// bytecode, runtime bytecode, and ABI.
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::{artifacts::*, Project};
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let output = project.compile()?.into_output();
    /// let contract = output.get("src/Greeter.sol".as_ref(), "Greeter").unwrap();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn get(&self, path: &Path, contract: &str) -> Option<CompactContractRef<'_>> {
        self.contracts.get(path, contract)
    }

    /// Returns the output's source files and contracts separately, wrapped in helper types that
    /// provide several helper methods
    ///
    /// # Examples
    /// ```no_run
    /// use foundry_compilers::Project;
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let output = project.compile()?.into_output();
    /// let (sources, contracts) = output.split();
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn split(self) -> (VersionedSourceFiles, VersionedContracts<C::CompilerContract>) {
        (self.sources, self.contracts)
    }

    /// Joins all file path with `root`
    pub fn join_all(&mut self, root: &Path) -> &mut Self {
        self.contracts.join_all(root);
        self.sources.join_all(root);
        self
    }

    /// Strips the given prefix from all file paths to make them relative to the given
    /// `base` argument.
    ///
    /// Convenience method for [Self::strip_prefix_all()] that consumes the type.
    ///
    /// # Examples
    ///
    /// Make all sources and contracts relative to the project's root directory
    /// ```no_run
    /// use foundry_compilers::Project;
    ///
    /// let project = Project::builder().build(Default::default())?;
    /// let output = project.compile()?.into_output().with_stripped_file_prefixes(project.root());
    /// # Ok::<_, Box<dyn std::error::Error>>(())
    /// ```
    pub fn with_stripped_file_prefixes(mut self, base: &Path) -> Self {
        self.contracts.strip_prefix_all(base);
        self.sources.strip_prefix_all(base);
        self
    }

    /// Removes `base` from all contract paths
    pub fn strip_prefix_all(&mut self, base: &Path) -> &mut Self {
        self.contracts.strip_prefix_all(base);
        self.sources.strip_prefix_all(base);
        self
    }
}

impl<C: Compiler> AggregatedCompilerOutput<C> {
    /// Whether the output contains a compiler error
    ///
    /// This adheres to the given `compiler_severity_filter` and also considers [CompilationError]
    /// with the given [Severity] as errors. For example [Severity::Warning] will consider
    /// [CompilationError]s with [Severity::Warning] and [Severity::Error] as errors.
    pub fn has_error(
        &self,
        ignored_error_codes: &[u64],
        ignored_file_paths: &[PathBuf],
        compiler_severity_filter: &Severity,
        seismic_warnings_in_tests: bool,
        no_seismic_warnings: bool,
    ) -> bool {
        self.errors.iter().any(|err| {
            if err.is_error() {
                // [Severity::Error] is always treated as an error
                return true;
            }
            // check if the filter is set to something higher than the error's severity
            if compiler_severity_filter.ge(&err.severity()) {
                if compiler_severity_filter.is_warning() {
                    // skip ignored error codes and file path from warnings
                    return self.has_warning(
                        ignored_error_codes,
                        ignored_file_paths,
                        seismic_warnings_in_tests,
                        no_seismic_warnings,
                    );
                }
                return true;
            }
            false
        })
    }

    /// Checks if there are any compiler warnings that are not ignored by the specified error codes
    /// and file paths.
    pub fn has_warning(
        &self,
        ignored_error_codes: &[u64],
        ignored_file_paths: &[PathBuf],
        seismic_warnings_in_tests: bool,
        no_seismic_warnings: bool,
    ) -> bool {
        self.errors.iter().any(|error| {
            !self.should_ignore(
                ignored_error_codes,
                ignored_file_paths,
                error,
                seismic_warnings_in_tests,
                no_seismic_warnings,
            )
        })
    }

    pub fn should_ignore(
        &self,
        ignored_error_codes: &[u64],
        ignored_file_paths: &[PathBuf],
        error: &C::CompilationError,
        seismic_warnings_in_tests: bool,
        no_seismic_warnings: bool,
    ) -> bool {
        if !error.is_warning() {
            return false;
        }

        let mut ignore = false;

        if let Some(code) = error.error_code() {
            // If no_seismic_warnings is set, suppress ALL seismic warnings globally.
            if no_seismic_warnings && code >= SEISMIC_WARNING_THRESHOLD {
                return true;
            }

            ignore |= ignored_error_codes.contains(&code);
            if let Some(loc) = error.source_location() {
                let path = Path::new(&loc.file);
                ignore |=
                    ignored_file_paths.iter().any(|ignored_path| path.starts_with(ignored_path));

                // we ignore spdx and contract size warnings in test
                // files. if we are looking at one of these warnings
                // from a test file we skip
                ignore |= self.is_test(path) && (code == 1878 || code == 5574);

                // Suppress shielded literal warnings that are safe in test/script files.
                // Constructor param (10103) and new-expression (10401,10404,10407,10410,10413)
                // warnings are NOT suppressed because those contracts ARE deployed
                // on-chain.
                //
                // Scripts always suppress. Tests suppress unless
                // seismic_warnings_in_tests is set.
                if SHIELDED_WARNINGS_SUPPRESSIBLE_IN_TESTS.contains(&code) {
                    ignore |= self.is_script(path);
                    if !seismic_warnings_in_tests {
                        ignore |= self.is_test(path);
                    }
                }
            }
        }

        ignore
    }

    /// Returns true if the contract is expected to be a script
    fn is_script(&self, contract_path: &Path) -> bool {
        let path_str = contract_path.to_string_lossy();
        if path_str.ends_with(".s.sol") {
            return true;
        }
        contract_path.components().any(|c| {
            let s = c.as_os_str().to_string_lossy();
            s == "script" || s == "scripts"
        })
    }

    /// Returns true if the contract is a expected to be a test
    fn is_test(&self, contract_path: &Path) -> bool {
        if contract_path.to_string_lossy().ends_with(".t.sol") {
            return true;
        }

        self.contracts.contracts_with_files().filter(|(path, _, _)| *path == contract_path).any(
            |(_, _, contract)| {
                contract.abi_ref().is_some_and(|abi| abi.functions.contains_key("IS_TEST"))
            },
        )
    }
}

/// Helper type to implement display for solc errors
#[derive(Clone, Debug)]
pub struct OutputDiagnostics<'a, C: Compiler> {
    /// output of the compiled project
    compiler_output: &'a AggregatedCompilerOutput<C>,
    /// the error codes to ignore
    ignored_error_codes: &'a [u64],
    /// the file paths to ignore
    ignored_file_paths: &'a [PathBuf],
    /// set minimum level of severity that is treated as an error
    compiler_severity_filter: Severity,
    /// When true, show seismic warnings (code >= 10000) even in test files.
    seismic_warnings_in_tests: bool,
    /// When true, suppress ALL seismic warnings (code >= 10000) globally.
    no_seismic_warnings: bool,
}

impl<C: Compiler> OutputDiagnostics<'_, C> {
    /// Returns true if there is at least one error of high severity
    pub fn has_error(&self) -> bool {
        self.compiler_output.has_error(
            self.ignored_error_codes,
            self.ignored_file_paths,
            &self.compiler_severity_filter,
            self.seismic_warnings_in_tests,
            self.no_seismic_warnings,
        )
    }

    /// Returns true if there is at least one warning
    pub fn has_warning(&self) -> bool {
        self.compiler_output.has_warning(
            self.ignored_error_codes,
            self.ignored_file_paths,
            self.seismic_warnings_in_tests,
            self.no_seismic_warnings,
        )
    }
}

impl<C: Compiler> fmt::Display for OutputDiagnostics<'_, C> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Compiler run ")?;
        if self.has_error() {
            write!(f, "{}:", "failed".red())
        } else if self.has_warning() {
            write!(f, "{}:", "successful with warnings".yellow())
        } else {
            write!(f, "{}!", "successful".green())
        }?;

        for err in &self.compiler_output.errors {
            if !self.compiler_output.should_ignore(
                self.ignored_error_codes,
                self.ignored_file_paths,
                err,
                self.seismic_warnings_in_tests,
                self.no_seismic_warnings,
            ) {
                f.write_str("\n")?;
                fmt::Display::fmt(&err, f)?;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compilers::multi::MultiCompilerError;
    use foundry_compilers_artifacts::Error as SolcDiagnostic;

    /// Build a minimal `AggregatedCompilerOutput<MultiCompiler>` for testing.
    fn make_output() -> AggregatedCompilerOutput<MultiCompiler> {
        AggregatedCompilerOutput {
            errors: vec![],
            sources: Default::default(),
            contracts: Default::default(),
            build_infos: Default::default(),
        }
    }

    /// Create a solc warning with a given error code originating from a given file.
    fn make_warning(code: u64, file: &str) -> MultiCompilerError {
        let raw = serde_json::json!({
            "type": "Warning",
            "component": "general",
            "severity": "warning",
            "errorCode": code.to_string(),
            "message": "test warning",
            "formattedMessage": "test warning",
            "sourceLocation": {
                "file": file,
                "start": 0,
                "end": 1
            }
        });
        let diag: SolcDiagnostic = serde_json::from_value(raw).expect("valid mock warning");
        MultiCompilerError::Solc(diag)
    }

    /// Create a solc error (non-warning) with a given error code from a given file.
    fn make_error(code: u64, file: &str) -> MultiCompilerError {
        let raw = serde_json::json!({
            "type": "Error",
            "component": "general",
            "severity": "error",
            "errorCode": code.to_string(),
            "message": "test error",
            "formattedMessage": "test error",
            "sourceLocation": {
                "file": file,
                "start": 0,
                "end": 1
            }
        });
        let diag: SolcDiagnostic = serde_json::from_value(raw).expect("valid mock error");
        MultiCompilerError::Solc(diag)
    }

    /// Helper: returns true if the warning would be suppressed (ignored).
    fn is_suppressed(code: u64, file: &str) -> bool {
        let output = make_output();
        let warning = make_warning(code, file);
        output.should_ignore(&[], &[], &warning, false, false)
    }

    /// Helper: returns true if the warning would be suppressed with given seismic flags.
    fn is_suppressed_with_flags(
        code: u64,
        file: &str,
        seismic_warnings_in_tests: bool,
        no_seismic_warnings: bool,
    ) -> bool {
        let output = make_output();
        let warning = make_warning(code, file);
        output.should_ignore(&[], &[], &warning, seismic_warnings_in_tests, no_seismic_warnings)
    }

    // src/ files: nothing suppressed

    #[test]
    fn src_file_shows_all_warnings() {
        // Constructor param
        assert!(!is_suppressed(SHIELDED_CONSTRUCTOR_PARAM, "src/Foo.sol"));
        // New-expression warnings (10401,10404,10407,10410,10413)
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_INT, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_BOOL, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_ADDRESS, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_FIXEDBYTES, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_ENUM, "src/Foo.sol"));
        // External call warnings (10402,10405,10408,10411,10414)
        assert!(!is_suppressed(SHIELDED_LITERAL_EXT_CALL_INT, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_EXT_CALL_BOOL, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_EXT_CALL_ADDRESS, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_EXT_CALL_FIXEDBYTES, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_EXT_CALL_ENUM, "src/Foo.sol"));
        // Other context (9660–9663, 1457)
        assert!(!is_suppressed(SHIELDED_LITERAL_OTHER_INT, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_OTHER_BOOL, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_OTHER_ADDRESS, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_OTHER_FIXEDBYTES, "src/Foo.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_OTHER_ENUM, "src/Foo.sol"));
    }

    // test/ files

    #[test]
    fn test_file_shows_constructor_param_warning() {
        assert!(!is_suppressed(SHIELDED_CONSTRUCTOR_PARAM, "test/Foo.t.sol"));
    }

    #[test]
    fn test_file_shows_new_expr_warnings() {
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_INT, "test/Foo.t.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_BOOL, "test/Foo.t.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_ADDRESS, "test/Foo.t.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_FIXEDBYTES, "test/Foo.t.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_ENUM, "test/Foo.t.sol"));
    }

    #[test]
    fn test_file_suppresses_ext_call_warnings() {
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_INT, "test/Foo.t.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_BOOL, "test/Foo.t.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_ADDRESS, "test/Foo.t.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_FIXEDBYTES, "test/Foo.t.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_ENUM, "test/Foo.t.sol"));
    }

    #[test]
    fn test_file_suppresses_other_context_warnings() {
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_INT, "test/Foo.t.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_BOOL, "test/Foo.t.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_ADDRESS, "test/Foo.t.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_FIXEDBYTES, "test/Foo.t.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_ENUM, "test/Foo.t.sol"));
    }

    // script/ files

    #[test]
    fn script_file_shows_constructor_param_warning() {
        assert!(!is_suppressed(SHIELDED_CONSTRUCTOR_PARAM, "script/Deploy.s.sol"));
    }

    #[test]
    fn script_file_shows_new_expr_warnings() {
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_INT, "script/Deploy.s.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_BOOL, "script/Deploy.s.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_ADDRESS, "script/Deploy.s.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_FIXEDBYTES, "script/Deploy.s.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_ENUM, "script/Deploy.s.sol"));
    }

    #[test]
    fn script_file_suppresses_ext_call_warnings() {
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_INT, "script/Deploy.s.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_BOOL, "script/Deploy.s.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_ADDRESS, "script/Deploy.s.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_FIXEDBYTES, "script/Deploy.s.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_ENUM, "script/Deploy.s.sol"));
    }

    #[test]
    fn script_file_suppresses_other_context_warnings() {
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_INT, "script/Deploy.s.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_BOOL, "script/Deploy.s.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_ADDRESS, "script/Deploy.s.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_FIXEDBYTES, "script/Deploy.s.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_ENUM, "script/Deploy.s.sol"));
    }

    // scripts/ directory variant

    #[test]
    fn scripts_dir_suppresses_same_as_script() {
        assert!(is_suppressed(SHIELDED_LITERAL_EXT_CALL_INT, "scripts/Deploy.s.sol"));
        assert!(is_suppressed(SHIELDED_LITERAL_OTHER_INT, "scripts/Deploy.s.sol"));
        assert!(!is_suppressed(SHIELDED_CONSTRUCTOR_PARAM, "scripts/Deploy.s.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_INT, "scripts/Deploy.s.sol"));
    }

    // lib/ and other paths: no suppression

    #[test]
    fn lib_file_shows_all_warnings() {
        assert!(!is_suppressed(SHIELDED_CONSTRUCTOR_PARAM, "lib/Dep.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_NEW_EXPR_INT, "lib/Dep.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_EXT_CALL_INT, "lib/Dep.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_OTHER_INT, "lib/Dep.sol"));
        assert!(!is_suppressed(SHIELDED_LITERAL_OTHER_ENUM, "lib/Dep.sol"));
    }

    // non-warning errors are never suppressed

    #[test]
    fn errors_are_never_suppressed() {
        let output = make_output();
        let error = make_error(SHIELDED_LITERAL_OTHER_INT, "test/Foo.t.sol");
        assert!(!output.should_ignore(&[], &[], &error, false, false));
    }

    // constant values are correct

    #[test]
    fn constant_values_match_spec() {
        assert_eq!(SHIELDED_CONSTRUCTOR_PARAM, 10103);
        assert_eq!(SHIELDED_LITERAL_NEW_EXPR_INT, 10401);
        assert_eq!(SHIELDED_LITERAL_NEW_EXPR_BOOL, 10404);
        assert_eq!(SHIELDED_LITERAL_NEW_EXPR_ADDRESS, 10407);
        assert_eq!(SHIELDED_LITERAL_NEW_EXPR_FIXEDBYTES, 10410);
        assert_eq!(SHIELDED_LITERAL_NEW_EXPR_ENUM, 10413);
        assert_eq!(SHIELDED_LITERAL_EXT_CALL_INT, 10402);
        assert_eq!(SHIELDED_LITERAL_EXT_CALL_BOOL, 10405);
        assert_eq!(SHIELDED_LITERAL_EXT_CALL_ADDRESS, 10408);
        assert_eq!(SHIELDED_LITERAL_EXT_CALL_FIXEDBYTES, 10411);
        assert_eq!(SHIELDED_LITERAL_EXT_CALL_ENUM, 10414);
        assert_eq!(SHIELDED_LITERAL_OTHER_INT, 10403);
        assert_eq!(SHIELDED_LITERAL_OTHER_BOOL, 10406);
        assert_eq!(SHIELDED_LITERAL_OTHER_ADDRESS, 10409);
        assert_eq!(SHIELDED_LITERAL_OTHER_FIXEDBYTES, 10412);
        assert_eq!(SHIELDED_LITERAL_OTHER_ENUM, 10415);
    }

    #[test]
    fn suppressible_collection_has_correct_entries() {
        // Should contain ext call + other context = 10 entries
        assert_eq!(SHIELDED_WARNINGS_SUPPRESSIBLE_IN_TESTS.len(), 10);
        // Should NOT contain constructor param or new-expression warnings
        assert!(!SHIELDED_WARNINGS_SUPPRESSIBLE_IN_TESTS.contains(&SHIELDED_CONSTRUCTOR_PARAM));
        assert!(!SHIELDED_WARNINGS_SUPPRESSIBLE_IN_TESTS.contains(&SHIELDED_LITERAL_NEW_EXPR_INT));
        assert!(!SHIELDED_WARNINGS_SUPPRESSIBLE_IN_TESTS.contains(&SHIELDED_LITERAL_NEW_EXPR_BOOL));
        assert!(
            !SHIELDED_WARNINGS_SUPPRESSIBLE_IN_TESTS.contains(&SHIELDED_LITERAL_NEW_EXPR_ADDRESS)
        );
        assert!(!SHIELDED_WARNINGS_SUPPRESSIBLE_IN_TESTS
            .contains(&SHIELDED_LITERAL_NEW_EXPR_FIXEDBYTES));
        assert!(!SHIELDED_WARNINGS_SUPPRESSIBLE_IN_TESTS.contains(&SHIELDED_LITERAL_NEW_EXPR_ENUM));
    }

    // --no-seismic-warnings suppresses ALL seismic warnings globally

    #[test]
    fn no_seismic_warnings_suppresses_all_in_src() {
        assert!(is_suppressed_with_flags(SHIELDED_CONSTRUCTOR_PARAM, "src/Foo.sol", false, true));
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_NEW_EXPR_INT,
            "src/Foo.sol",
            false,
            true
        ));
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_EXT_CALL_INT,
            "src/Foo.sol",
            false,
            true
        ));
        assert!(is_suppressed_with_flags(SHIELDED_LITERAL_OTHER_INT, "src/Foo.sol", false, true));
    }

    #[test]
    fn no_seismic_warnings_suppresses_all_in_test() {
        assert!(is_suppressed_with_flags(
            SHIELDED_CONSTRUCTOR_PARAM,
            "test/Foo.t.sol",
            false,
            true
        ));
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_NEW_EXPR_INT,
            "test/Foo.t.sol",
            false,
            true
        ));
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_EXT_CALL_INT,
            "test/Foo.t.sol",
            false,
            true
        ));
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_OTHER_INT,
            "test/Foo.t.sol",
            false,
            true
        ));
    }

    #[test]
    fn no_seismic_warnings_does_not_suppress_non_seismic() {
        // Warning code below threshold should not be affected
        assert!(!is_suppressed_with_flags(1878, "src/Foo.sol", false, true));
        assert!(!is_suppressed_with_flags(5574, "src/Foo.sol", false, true));
    }

    #[test]
    fn no_seismic_warnings_errors_never_suppressed() {
        let output = make_output();
        let error = make_error(SHIELDED_LITERAL_OTHER_INT, "src/Foo.sol");
        assert!(!output.should_ignore(&[], &[], &error, false, true));
    }

    // --seismic-warnings-in-tests disables test suppression (not script)

    #[test]
    fn seismic_warnings_in_tests_shows_ext_call_in_test() {
        // Normally suppressed in test files
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_EXT_CALL_INT,
            "test/Foo.t.sol",
            false,
            false
        ));
        // With flag, no longer suppressed
        assert!(!is_suppressed_with_flags(
            SHIELDED_LITERAL_EXT_CALL_INT,
            "test/Foo.t.sol",
            true,
            false
        ));
    }

    #[test]
    fn seismic_warnings_in_tests_shows_other_in_test() {
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_OTHER_INT,
            "test/Foo.t.sol",
            false,
            false
        ));
        assert!(!is_suppressed_with_flags(
            SHIELDED_LITERAL_OTHER_INT,
            "test/Foo.t.sol",
            true,
            false
        ));
    }

    #[test]
    fn seismic_warnings_in_tests_no_effect_on_script() {
        // Scripts always suppress — flag only affects test files
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_EXT_CALL_INT,
            "script/Deploy.s.sol",
            false,
            false
        ));
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_EXT_CALL_INT,
            "script/Deploy.s.sol",
            true,
            false
        ));
    }

    #[test]
    fn seismic_warnings_in_tests_no_effect_on_src() {
        // src/ files already show all warnings; flag should not change behavior
        assert!(!is_suppressed_with_flags(
            SHIELDED_LITERAL_EXT_CALL_INT,
            "src/Foo.sol",
            false,
            false
        ));
        assert!(!is_suppressed_with_flags(
            SHIELDED_LITERAL_EXT_CALL_INT,
            "src/Foo.sol",
            true,
            false
        ));
    }

    // no_seismic_warnings takes precedence over seismic_warnings_in_tests

    #[test]
    fn no_seismic_warnings_takes_precedence() {
        // Both flags set: no_seismic_warnings should still suppress
        assert!(is_suppressed_with_flags(
            SHIELDED_LITERAL_EXT_CALL_INT,
            "test/Foo.t.sol",
            true,
            true
        ));
        assert!(is_suppressed_with_flags(SHIELDED_LITERAL_OTHER_INT, "src/Foo.sol", true, true));
    }

    #[test]
    fn seismic_warning_threshold_is_correct() {
        assert_eq!(SEISMIC_WARNING_THRESHOLD, 10000);
    }
}

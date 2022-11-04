mod cli;
mod utils;

use cargo_toml::{Dependency, DepsSet, Manifest};
use clap::Parser;
use cli::{Cargo, DylibCli};
use indoc::formatdoc;
use rayon::prelude::*;
use serde::Serialize;
use std::path::Path;
use std::process::{Command, Stdio};
use std::{fs, io};
use tap::Tap;
use tool::prelude::*;
use utils::inject;

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
    #[error(transparent)]
    CargoToml(#[from] cargo_toml::Error),
    #[error(transparent)]
    LocateManifest(#[from] locate_cargo_manifest::LocateManifestError),
}

fn main() -> Result<std::process::ExitCode, Error> {
    human_panic::setup_panic!();

    let Cargo::Dylib(cli) = Cargo::parse();

    let (real_manifest_path, dylib_path, dylib_manifest_path) = get_paths()?;

    init_dylibs(&real_manifest_path, &dylib_path, &dylib_manifest_path)?;

    invoke_cargo(&cli, &dylib_manifest_path).map_err(Error::Io)
}

fn get_paths() -> Result<(std::path::PathBuf, std::path::PathBuf, std::path::PathBuf), Error> {
    let real_manifest_path = locate_cargo_manifest::locate_manifest()?;
    let dylib_path = real_manifest_path
        .clone()
        .tap_mut(|real| {
            real.pop();
        })
        .tap_mut(|root| root.extend(["target", "cargo-dylib"]));
    let dylib_manifest_path = dylib_path.join("Cargo.toml");
    Ok((real_manifest_path, dylib_path, dylib_manifest_path))
}

#[derive(Debug, Serialize)]
struct ManifestDependencies {
    pub dependencies: DepsSet,
}

impl ManifestDependencies {
    fn from_dependency(dep: (&String, &Dependency)) -> Self {
        let mut subdependencies = DepsSet::new();
        subdependencies.insert(dep.0.clone(), dep.1.clone());
        Self {
            dependencies: subdependencies,
        }
    }
}

fn init_dylibs(
    real_manifest_path: &Path,
    dylib_path: &Path,
    dylib_manifest_path: &Path,
) -> Result<(), Error> {
    if has_manifest_changed(real_manifest_path, dylib_manifest_path)? {
        return Ok(());
    }

    fs::DirBuilder::new().recursive(true).create(dylib_path)?;

    let dylib_manifest = create_dylibs_with_manifest(real_manifest_path, dylib_path)?;

    write_dylib_manifest(dylib_manifest, dylib_manifest_path)?;

    Ok(())
}

fn has_manifest_changed(
    real_manifest_path: &Path,
    dylib_manifest_path: &Path,
) -> Result<bool, Error> {
    let real_manifest_modified = fs::metadata(real_manifest_path)?.modified()?;
    let dylib_manifest_modified = fs::metadata(dylib_manifest_path)
        .ok()
        .map(|metadata| metadata.modified())
        .transpose()?;

    Ok(dylib_manifest_modified
        .map(|modified| real_manifest_modified < modified)
        .unwrap_or(false))
}

fn create_dylibs_with_manifest(
    real_manifest_path: &Path,
    dylib_path: &Path,
) -> Result<Manifest, Error> {
    let real_manifest = Manifest::from_path(real_manifest_path)?;
    let mut dylib_manifest = real_manifest.clone();

    dylib_manifest.dependencies = real_manifest
        .dependencies
        .par_iter()
        .map(inject(init_dep, dylib_path))
        .collect::<Result<_, _>>()?;

    dylib_manifest.bin.first_mut().unwrap().path = Some("../../src/main.rs".to_string());

    Ok(dylib_manifest)
}

fn write_dylib_manifest(dylib_manifest: Manifest, dylib_manifest_path: &Path) -> Result<(), Error> {
    let dylib_manifest = toml::to_string(&dylib_manifest)?;
    std::fs::write(dylib_manifest_path, dylib_manifest).map_err(Error::Io)
}

fn init_dep(dep: (&String, &Dependency), dylib_path: &Path) -> Result<(String, Dependency), Error> {
    let dynamic_name = format!("{}-dynamic", dep.0);
    let dynamic_crate_path = dylib_path.join(&dynamic_name);

    let subdependency = subdependency(&dynamic_name, dep);

    if std::path::Path::new(&dynamic_crate_path).exists() {
        return Ok(subdependency);
    }

    let dynamic_crate_src = dynamic_crate_path.join("src");
    fs::DirBuilder::new()
        .recursive(true)
        .create(&dynamic_crate_src)?;

    let subdependencies = ManifestDependencies::from_dependency(dep);
    let dynamic_manifest = create_dynamic_manifest(subdependencies, &dynamic_name)?;

    write_dynamic_manifest(dynamic_crate_path, dynamic_manifest)?;

    write_dynamic_src(dynamic_crate_src, dep)?;

    Ok(subdependency)
}

fn subdependency(dynamic_name: &str, dep: (&String, &Dependency)) -> (String, Dependency) {
    let dep_detail = cargo_toml::DependencyDetail {
        path: Some(dynamic_name.to_string()),
        package: Some(dynamic_name.to_string()),
        ..Default::default()
    };
    (dep.0.clone(), Dependency::Detailed(dep_detail))
}

fn create_dynamic_manifest(
    subdependencies: ManifestDependencies,
    dynamic_name: &str,
) -> Result<String, Error> {
    let subdependencies = toml::to_string(&subdependencies)?;
    let dynamic_manifest = formatdoc!(
        "
        [package]
        name = '{dynamic_name}'
        version = '0.1.0'
        edition = '2021'
    
        {subdependencies}

        [lib]
        crate-type = ['dylib']
        "
    );
    Ok(dynamic_manifest)
}

fn write_dynamic_manifest(
    dynamic_crate_path: std::path::PathBuf,
    dynamic_manifest: String,
) -> Result<(), io::Error> {
    std::fs::write(dynamic_crate_path.join("Cargo.toml"), dynamic_manifest)
}

fn write_dynamic_src(
    dynamic_crate_src: std::path::PathBuf,
    dep: (&String, &Dependency),
) -> Result<(), io::Error> {
    std::fs::write(
        dynamic_crate_src.join("lib.rs"),
        format!("pub use {}::*;", dep.0),
    )
}

fn invoke_cargo(
    cli: &DylibCli,
    dylib_manifest_path: &Path,
) -> std::io::Result<std::process::ExitCode> {
    Command::new("cargo")
        .arg(&cli.subcommand)
        .arg("--manifest-path")
        .arg(dylib_manifest_path)
        .args(&cli.arguments)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()?
        .wait()
        .map(status_to_exitcode)
}

fn status_to_exitcode(status: std::process::ExitStatus) -> std::process::ExitCode {
    let i32_to_u8 = compose(ok, u8::try_from);

    status.code().and_then(i32_to_u8).unwrap_or(1).into()
}

mod cli;
mod utils;

use cargo_toml::{Dependency, DepsSet, Manifest};
use clap::Parser;
use cli::{Cargo, DylibCli};
use indoc::formatdoc;
use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::process::{Command, Stdio};
use tap::Tap;
use tool::prelude::*;
use utils::inject;

#[derive(thiserror::Error, Debug)]
enum Error {
    #[error(transparent)]
    Io(#[from] std::io::Error),
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

fn init_dylibs(
    real_manifest_path: &Path,
    dylib_path: &Path,
    dylib_manifest_path: &Path,
) -> Result<(), Error> {
    if has_manifest_changed(real_manifest_path, dylib_manifest_path)? {
        return Ok(());
    }

    let real_manifest = Manifest::from_path(real_manifest_path)?;
    let mut dylib_manifest = real_manifest.clone();

    fs::DirBuilder::new().recursive(true).create(dylib_path)?;

    dylib_manifest.dependencies = real_manifest
        .dependencies
        .par_iter()
        .map(inject(init_dep, dylib_path))
        .collect::<Result<_, _>>()?;

    dylib_manifest.bin.first_mut().unwrap().path = Some("../../src/main.rs".to_string());

    let dylib_manifest = toml::to_string(&dylib_manifest)?;
    std::fs::write(dylib_manifest_path, dylib_manifest)?;

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

fn init_dep(dep: (&String, &Dependency), dylib_path: &Path) -> Result<(String, Dependency), Error> {
    let dynamic_name = format!("{}-dynamic", dep.0);
    let dynamic_crate_path = dylib_path.join(&dynamic_name);

    let dep_detail = cargo_toml::DependencyDetail {
        path: Some(dynamic_name.clone()),
        package: Some(dynamic_name.clone()),
        ..Default::default()
    };

    let dependency = (dep.0.clone(), Dependency::Detailed(dep_detail));

    if std::path::Path::new(&dynamic_crate_path).exists() {
        return Ok(dependency);
    }

    let dynamic_crate_src = dynamic_crate_path.join("src");
    fs::DirBuilder::new()
        .recursive(true)
        .create(&dynamic_crate_src)?;

    let mut dynamic_dependencies = DepsSet::new();
    dynamic_dependencies.insert(dep.0.clone(), dep.1.clone());
    let dynamic_dependencies = ManifestDependencies {
        dependencies: dynamic_dependencies,
    };
    let dynamic_dependencies = toml::to_string(&dynamic_dependencies)?;

    let dynamic_manifest = formatdoc!(
        "
        [package]
        name = '{dynamic_name}'
        version = '0.1.0'
        edition = '2021'
    
        {dynamic_dependencies}

        [lib]
        crate-type = ['dylib']
        "
    );

    std::fs::write(dynamic_crate_path.join("Cargo.toml"), dynamic_manifest)?;

    std::fs::write(
        dynamic_crate_src.join("lib.rs"),
        format!("pub use {}::*;", dep.0),
    )?;

    Ok(dependency)
}

fn invoke_cargo(
    cli: &DylibCli,
    dylib_manifest_path: &Path,
) -> std::io::Result<std::process::ExitCode> {
    let status_to_u8 = compose(ok, u8::try_from);

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
        .map(|status| status.code().and_then(status_to_u8).unwrap_or(1).into())
}

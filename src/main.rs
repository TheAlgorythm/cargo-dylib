mod cli;

use cargo_toml::{Dependency, DepsSet, Manifest};
use clap::Parser;
use cli::{Cargo, DylibCli};
use indoc::formatdoc;
use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::process::{Command, Stdio};

const DYNLIB_PATH: &str = "target/cargo-dylib/";
const DYNLIB_MANIFEST_PATH: &str = "target/cargo-dylib/Cargo.toml";

fn main() {
    let Cargo::Dylib(cli) = Cargo::parse();

    init_dylibs();

    invoke_cargo(&cli);
}

#[derive(Debug, Serialize)]
struct ManifestDependencies {
    pub dependencies: DepsSet,
}

fn init_dylibs() {
    let real_manifest_path = "Cargo.toml";

    let real_manifest_modified = fs::metadata(real_manifest_path)
        .unwrap()
        .modified()
        .unwrap();
    let dylib_manifest_modified = fs::metadata(DYNLIB_MANIFEST_PATH)
        .ok()
        .map(|metadata| metadata.modified())
        .transpose()
        .unwrap();
    if dylib_manifest_modified
        .map(|modified| real_manifest_modified < modified)
        .unwrap_or(false)
    {
        return;
    }

    let real_manifest = Manifest::from_path(real_manifest_path).unwrap();
    let mut dylib_manifest = real_manifest.clone();

    fs::DirBuilder::new()
        .recursive(true)
        .create(DYNLIB_PATH)
        .unwrap();

    dylib_manifest.dependencies = real_manifest
        .dependencies
        .par_iter()
        .map(init_dep)
        .collect();

    dylib_manifest.bin.first_mut().unwrap().path = Some("../../src/main.rs".to_string());

    let dylib_manifest = toml::to_string(&dylib_manifest).unwrap();
    std::fs::write(DYNLIB_MANIFEST_PATH, dylib_manifest).unwrap();
}

fn init_dep(dep: (&String, &Dependency)) -> (String, Dependency) {
    let dynamic_name = format!("{}-dynamic", dep.0);
    let dynamic_crate_path = format!("{DYNLIB_PATH}{dynamic_name}");

    let dep_detail = cargo_toml::DependencyDetail {
        path: Some(dynamic_name.clone()),
        package: Some(dynamic_name.clone()),
        ..Default::default()
    };

    let dependency = (dep.0.clone(), Dependency::Detailed(dep_detail));

    if std::path::Path::new(&dynamic_crate_path).exists() {
        return dependency;
    }

    fs::DirBuilder::new()
        .recursive(true)
        .create(format!("{dynamic_crate_path}/src"))
        .unwrap();

    let mut dynamic_dependencies = DepsSet::new();
    dynamic_dependencies.insert(dep.0.clone(), dep.1.clone());
    let dynamic_dependencies = ManifestDependencies {
        dependencies: dynamic_dependencies,
    };
    let dynamic_dependencies = toml::to_string(&dynamic_dependencies).unwrap();

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

    std::fs::write(format!("{dynamic_crate_path}/Cargo.toml"), dynamic_manifest).unwrap();

    std::fs::write(
        format!("{dynamic_crate_path}/src/lib.rs"),
        format!("pub use {}::*;", dep.0),
    )
    .unwrap();

    dependency
}

fn invoke_cargo(cli: &DylibCli) {
    Command::new("cargo")
        .arg(&cli.subcommand)
        .arg("--manifest-path")
        .arg(DYNLIB_MANIFEST_PATH)
        .args(&cli.arguments)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
}

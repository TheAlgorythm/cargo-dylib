mod cli;

use cargo_toml::{Dependency, DepsSet, Manifest};
use clap::Parser;
use cli::{Cargo, SubCommand};
use indoc::formatdoc;
use rayon::prelude::*;
use std::fs::DirBuilder;
use std::process::{Command, Stdio};

const DYNLIB_PATH: &'static str = "target/cargo-dylib/";
const DYNLIB_MANIFEST_PATH: &'static str = "target/cargo-dylib/Cargo.toml";

fn main() {
    let Cargo::Dylib(cli) = Cargo::parse();

    match cli.subcommand {
        SubCommand::Init => init_dylibs(),
        SubCommand::Build => build(),
        SubCommand::Run => run(),
    }
}

fn init_dylibs() {
    let real_manifest_path = "Cargo.toml";
    let real_manifest = Manifest::from_path(real_manifest_path).unwrap();
    let mut dylib_manifest = real_manifest.clone();

    DirBuilder::new()
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

    let mut dep_detail = cargo_toml::DependencyDetail::default();
    dep_detail.path = Some(dynamic_name.clone());
    dep_detail.package = Some(dynamic_name.clone());

    let dependency = (dep.0.clone(), Dependency::Detailed(dep_detail));

    if std::path::Path::new(&dynamic_crate_path).exists() {
        return dependency;
    }

    DirBuilder::new()
        .recursive(true)
        .create(format!("{dynamic_crate_path}/src"))
        .unwrap();

    let mut deps = DepsSet::new();
    deps.insert(dep.0.clone(), dep.1.clone());
    let deps = toml::to_string(&deps).unwrap();
    let deps_no_open_bracket = deps.get(1..).unwrap();

    let dynamic_manifest = formatdoc!(
        "
        [package]
        name = '{dynamic_name}'
        version = '0.1.0'
        edition = '2021'
    
        [dependencies.{deps_no_open_bracket}

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

fn build() {
    init_dylibs();

    Command::new("cargo")
        .arg("build")
        .arg("--manifest-path")
        .arg(DYNLIB_MANIFEST_PATH)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
}

fn run() {
    Command::new("cargo")
        .arg("run")
        .arg("--manifest-path")
        .arg(DYNLIB_MANIFEST_PATH)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .spawn()
        .unwrap()
        .wait()
        .unwrap();
}

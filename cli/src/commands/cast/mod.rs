use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;

use clap::ArgMatches;
use path_abs::PathInfo;
use registry_common::models::GetIomodAtResponse;
use reqwest;

use assemblylift_core::wasm;

use crate::archive;
use crate::commands::cast::rust::RustFunction;
use crate::projectfs::Project;
use crate::terraform;
use crate::tools;
use crate::transpiler::context::Context;
use crate::transpiler::toml::service::Function;
use crate::transpiler::{context, toml, Castable};

mod ruby;
mod rust;

pub trait CastableFunction {
    fn compile(&self, wasi_snapshot_preview1: Vec<u8>);
    fn compose(&self);
    fn precompile(&self, target: Option<&str>);
    fn artifact_path(&self) -> PathBuf;
}

pub fn command(matches: Option<&ArgMatches>) {
    use std::rc::Rc;

    let _matches = match matches {
        Some(matches) => matches,
        _ => panic!("could not get matches for cast command"),
    };

    // Init the project structure -- panic if the project isn't in the current working dir
    let cwd = std::env::current_dir().unwrap();
    let mut manifest_path = cwd.clone();
    manifest_path.push("assemblylift.toml");

    let asml_manifest =
        toml::asml::Manifest::read(&manifest_path).expect("could not read assemblylift.toml");
    let project = Rc::new(Project::new(asml_manifest.project.name.clone(), Some(cwd)));

    let ctx = Rc::new(
        Context::from_project(project.clone(), asml_manifest)
            .expect("could not make context from manifest"),
    );

    let wasi_snapshot_preview1 = include_bytes!("wasm/wasi_snapshot_preview1.reactor.wasm");

    // Fetch the latest terraform binary to the project directory
    terraform::fetch(&*project.dir());

    // Compile WASM & package function
    let functions = ctx.functions.as_slice();
    for function in functions {
        let function_artifact_path = project
            .net_dir()
            .service_dir(&function.service_name.clone())
            .function_dir(function.name.clone())
            .into_os_string()
            .into_string()
            .unwrap();

        let castable_function: Box<dyn CastableFunction> = match function.language.clone().as_str()
        {
            "rust" => Box::new(RustFunction::new(&function)),
            // "ruby" => ruby::compile(project, service_name, function),
            _ => panic!("unsupported function language"),
        };
        castable_function.compile(wasi_snapshot_preview1.clone().to_vec());
        if function.precompile {
            // TODO set target triple
            castable_function.precompile(None);
        }

        // TODO zip not needed w/ container functions
        let mut function_dirs = vec![castable_function.artifact_path()];
        if "ruby" == function.language.clone().as_str() {
            function_dirs.push(PathBuf::from(format!(
                "{}/rubysrc",
                &function_artifact_path
            )));
        }
        archive::zip_dirs(
            function_dirs,
            format!("{}/{}.zip", function_artifact_path.clone(), &function.name),
            Vec::new(),
        )
        .expect("unable to zip function artifacts");
    }

    // Cast Context to artifacts
    {
        let artifacts = ctx
            .cast(ctx.clone(), None)
            .expect("could not cast assemblylift context");
        for artifact in artifacts {
            let path = artifact.write_path;
            let mut file = match fs::File::create(path.clone()) {
                Err(why) => panic!("couldn't create file {}: {}", path.clone(), why.to_string()),
                Ok(file) => file,
            };

            file.write_all(artifact.content.as_bytes())
                .expect("could not write artifact");
            println!("📄 > Wrote {}", path.clone());
        }
    }

    terraform::commands::init();
    terraform::commands::plan();
}

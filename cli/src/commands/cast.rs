use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process;
use std::process::Stdio;
use std::str::FromStr;

use wasmer::{Store, Module};
use wasmer_compiler::{CpuFeature, Target, Triple};
use wasmer_compiler_cranelift::Cranelift;
use wasmer_engine_native::Native;

use clap::ArgMatches;

use crate::artifact;
use crate::materials::{asml, hcl, toml, Artifact};
use crate::projectfs::Project;
use crate::terraform;

pub fn command(matches: Option<&ArgMatches>) {
    use std::io::Read;
    use std::rc::Rc;

    let _matches = match matches {
        Some(matches) => matches,
        _ => panic!("could not get matches for cast command"),
    };

    // Init the project structure -- panic if the project isn't in the current working dir
    let cwd = std::env::current_dir().unwrap();
    let asml_manifest = toml::asml::Manifest::read(&cwd).unwrap();
    let project = Rc::new(Project::new(asml_manifest.project.name.clone(), Some(cwd)));

    // Download the latest runtime binary
    let runtime_url = &*format!(
        "http://runtime.assemblylift.akkoro.io/aws-lambda/{}/bootstrap.zip",
//        clap::crate_version!(),
    "xlem",
    );
    let mut response = reqwest::blocking::get(runtime_url).unwrap();
    if !response.status().is_success() {
        panic!("unable to fetch asml runtime from {}", runtime_url);
    }
    let mut response_buffer = Vec::new();
    response.read_to_end(&mut response_buffer).unwrap();

    fs::create_dir_all("./.asml/runtime").unwrap();
    fs::write("./.asml/runtime/bootstrap.zip", response_buffer).unwrap();

    terraform::fetch(&*project.dir());

    let ctx = asml::Context::from_project(project.clone(), asml_manifest)
        .expect("could not make context from manifest");
    let mut module = hcl::root::Module::new(Rc::new(ctx));
    let hcl_content = module.cast().expect("could not cast HCL modules");
    println!("DEBUG: {}", hcl_content);
    
    let services = module.services.unwrap();
    for service in services {
        let service_name = service.name();

        if let Some(iomods) = service.iomods {
            let mut dependencies: Vec<String> = Vec::new();
            for (name, dependency) in iomods {
                match dependency.dependency_type.as_str() {
                    "file" => {
                        // copy file & rename it to `name`

                        let dependency_name = name.clone();

                        let runtime_path = format!("./.asml/runtime/{}", dependency_name);
                        match fs::metadata(dependency.from.clone()) {
                            Ok(_) => {
                                fs::copy(dependency.from.clone(), &runtime_path).unwrap();
                                ()
                            },
                            Err(_) => panic!("ERROR: could not find file-type dependency named {} (check path)", dependency_name),
                        }

                        dependencies.push(runtime_path);
                    }
                    _ => unimplemented!("only type=file is available currently"),
                }
            }

            artifact::zip_files(
                dependencies,
                format!("./.asml/runtime/{}.zip", &service_name),
                Some("iomod/"),
                false,
            );
        }

        if let Some(functions) = service.functions {
            for function_box in functions {
                let function = *function_box;
                let function_name = function.name();
                let function_artifact_path =
                    format!("./net/services/{}/{}", &service_name, function_name);
                fs::create_dir_all(PathBuf::from(function_artifact_path.clone())).expect(&*format!(
                    "unable to create path {}",
                    function_artifact_path
                ));

                // Compile the function
                // TODO switch on function language, toggle compilation on/off

                let function_path = PathBuf::from(format!(
                    "{}/Cargo.toml",
                    project
                        .clone()
                        .service_dir(service_name.clone())
                        .function_dir(function_name.clone())
                        .into_os_string()
                        .into_string()
                        .unwrap()
                ));

                let mode = "release"; // TODO should this really be the default?

                let mut cargo_build = process::Command::new("cargo")
                    .arg("build")
                    .arg(format!("--{}", mode))
                    .arg("--manifest-path")
                    .arg(function_path)
                    .arg("--target")
                    .arg("wasm32-unknown-unknown")
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap();

                match cargo_build.wait() {
                    Ok(_) => {}
                    Err(_) => {}
                }

                let function_name_snaked = function_name.replace("-", "_");
                let copy_result = fs::copy(
                    format!(
                        "{}/target/wasm32-unknown-unknown/{}/{}.wasm",
                        project
                            .clone()
                            .service_dir(service_name.clone())
                            .function_dir(function_name.clone())
                            .into_os_string()
                            .into_string()
                            .unwrap(),
                        mode,
                        function_name_snaked
                    ),
                    format!("{}/{}.wasm", function_artifact_path.clone(), &function_name),
                );

                if copy_result.is_err() {
                    println!("ERROR: {:?}", copy_result.err());
                }

                let wasm_path = format!("{}/{}.wasm", function_artifact_path.clone(), &function_name);
                let module_file_path = format!("{}/{}.wasm.bin", function_artifact_path.clone(), &function_name);

                let compiler = Cranelift::default();
                let triple = Triple::from_str("x86_64-linux-unknown").unwrap();
                let mut cpuid = CpuFeature::set();
                cpuid.insert(CpuFeature::from_str("sse2").unwrap());
                cpuid.insert(CpuFeature::from_str("avx2").unwrap());
                let store = Store::new(&Native::new(compiler)
                    .target(Target::new(triple, cpuid))
                    .engine()
                );

                let wasm_bytes = match fs::read(wasm_path) {
                    Ok(bytes) => bytes,
                    Err(err) => panic!(err.to_string()),
                };
                let module = Module::new(&store, wasm_bytes).unwrap();
                let module_bytes = module.serialize().unwrap();
                let mut module_file = match fs::File::create(module_file_path.clone()) {
                    Ok(file) => file,
                    Err(err) => panic!(err.to_string()),
                };
                println!("📄 > Wrote {}", module_file_path.clone());
                module_file.write_all(&module_bytes).unwrap();

                artifact::zip_files(
                    vec![module_file_path],
                    format!("{}/{}.zip", function_artifact_path.clone(), &function_name),
                    None,
                    false,
                );
            }
        }
    }

    //terraform::write(
    //    &*project.dir(),
    //    asml_manifest.project.name,
    //    functions,
    //    services,
    //)
    //.unwrap();

    terraform::commands::init();
    terraform::commands::plan();
}

// Copyright 2026 KONGSBERG
//
// Redistribution and use in source and binary forms, with or without
// modification, are permitted provided that the following conditions are met:
//
// 1. Redistributions of source code must retain the above copyright notice,
//    this list of conditions and the following disclaimer.
//
// 2. Redistributions in binary form must reproduce the above copyright notice,
//    this list of conditions and the following disclaimer in the documentation
//    and/or other materials provided with the distribution.
//
// 3. Neither the name of the copyright holder nor the names of its contributors
//    may be used to endorse or promote products derived from this software
//    without specific prior written permission.
//
// THIS SOFTWARE IS PROVIDED BY THE COPYRIGHT HOLDERS AND CONTRIBUTORS "AS IS" AND
// ANY EXPRESS OR IMPLIED WARRANTIES, INCLUDING, BUT NOT LIMITED TO, THE IMPLIED
// WARRANTIES OF MERCHANTABILITY AND FITNESS FOR A PARTICULAR PURPOSE ARE
// DISCLAIMED. IN NO EVENT SHALL THE COPYRIGHT HOLDER OR CONTRIBUTORS BE LIABLE
// FOR ANY DIRECT, INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, OR CONSEQUENTIAL
// DAMAGES (INCLUDING, BUT NOT LIMITED TO, PROCUREMENT OF SUBSTITUTE GOODS OR
// SERVICES; LOSS OF USE, DATA, OR PROFITS; OR BUSINESS INTERRUPTION) HOWEVER
// CAUSED AND ON ANY THEORY OF LIABILITY, WHETHER IN CONTRACT, STRICT LIABILITY,
// OR TORT (INCLUDING NEGLIGENCE OR OTHERWISE) ARISING IN ANY WAY OUT OF THE USE
// OF THIS SOFTWARE, EVEN IF ADVISED OF THE POSSIBILITY OF SUCH DAMAGE.

use std::collections::HashSet;
use std::num::NonZero;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Run integration tests
#[derive(ic_cli::Command, Default)]
pub struct Options {
    /// Languages to test: c, python, typescript, csharp, cpp, java, rust, pydantic
    #[option(short, long, arg = "lang")]
    pub lang: HashSet<String>,

    /// Path to ic-idl compiler executable
    #[option(long, arg = "path")]
    pub idl_compiler: Option<String>,
}

fn git_root() -> PathBuf {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .unwrap();

    PathBuf::from(std::str::from_utf8(&output.stdout).unwrap().trim())
}

fn run_command(mut cmd: Command, command_name: &str) {
    let status = match cmd.status() {
        Ok(status) => status,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("error: {command_name} not found");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("error: failed to run {command_name}: {e}");
            std::process::exit(1);
        }
    };

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
}

fn run_typescript_tests(integration_dir: &Path, idl_compiler: &Path) {
    let mut cmd = Command::new("bun");
    cmd.current_dir(integration_dir.join("typescript"))
        .env("IDL_COMPILER", idl_compiler)
        .arg("test");
    run_command(cmd, "bun");
}

fn run_csharp_tests(integration_dir: &Path, build_dir: &Path, idl_compiler: &Path) {
    let mut cmd = Command::new("dotnet");
    cmd.current_dir(integration_dir.join("csharp"))
        .env("IDL_COMPILER", idl_compiler)
        .args(["test", "--verbosity", "minimal", "--artifacts-path"])
        .arg(build_dir.join("csharp"));
    run_command(cmd, "dotnet");
}

fn run_cmake_tests(source_dir: &Path, build_dir: &Path, idl_compiler: &Path) {
    let jobs = std::thread::available_parallelism()
        .map_or(4, NonZero::get)
        .to_string();

    let mut cmd = Command::new("cmake");
    cmd.current_dir(source_dir)
        .args(["-S", ".", "-B"])
        .arg(build_dir)
        .arg(format!("-DIC_IDL_EXECUTABLE={}", idl_compiler.display()));
    run_command(cmd, "cmake");

    let mut cmd = Command::new("cmake");
    cmd.current_dir(source_dir)
        .arg("--build")
        .arg(build_dir)
        .args(["--target", "test", "-j", &jobs]);
    run_command(cmd, "cmake");
}

fn run_python_tests(integration_dir: &Path, build_dir: &Path, idl_compiler: &Path) {
    let build_dir = build_dir.join("python");
    std::fs::create_dir_all(&build_dir).unwrap();

    let mut cmd = Command::new("uv");
    cmd.current_dir(integration_dir.join("python"))
        .env("TMPDIR", &build_dir)
        .env("PYTHONPYCACHEPREFIX", build_dir.join("pycache"))
        .args(["run", "pytest", "-n", "auto"])
        .arg(format!("--idl-compiler={}", idl_compiler.display()))
        .arg(format!("--basetemp={}", build_dir.join("tmp").display()))
        .args(["-o"])
        .arg(format!(
            "cache_dir={}",
            build_dir.join("pytest-cache").display()
        ));
    run_command(cmd, "uv");
}

fn run_pydantic_tests(integration_dir: &Path) {
    let mut cmd = Command::new("uv");
    cmd.current_dir(integration_dir.join("pydantic"))
        .args(["run", "pytest", "-n", "auto"]);
    run_command(cmd, "uv");
}

fn run_java_tests(integration_dir: &Path, build_dir: &Path, idl_compiler: &Path) {
    let mut cmd = Command::new("mvn");
    cmd.current_dir(integration_dir.join("java"))
        .arg(format!(
            "-Dintegration.build.directory={}",
            build_dir.join("java").display()
        ))
        .arg(format!("-Didl.compiler={}", idl_compiler.display()))
        .arg("test");
    run_command(cmd, "mvn");
}

fn run_json_schema_tests(integration_dir: &Path, build_dir: &Path, idl_compiler: &Path) {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(integration_dir.join("json_schema"))
        .env("CARGO_TARGET_DIR", build_dir.join("rust"))
        .env("IDL_COMPILER", idl_compiler)
        .arg("test");
    run_command(cmd, "cargo");
}

fn run_rust_tests(integration_dir: &Path, build_dir: &Path, idl_compiler: &Path) {
    let mut cmd = Command::new("cargo");
    cmd.current_dir(integration_dir.join("rust"))
        .env("CARGO_TARGET_DIR", build_dir.join("rust"))
        .env("IDL_COMPILER", idl_compiler)
        .arg("test");
    run_command(cmd, "cargo");
}

pub fn run(opts: &Options) {
    let root = git_root();
    let integration_dir = root.join("integration-tests");
    let build_dir = root.join("target/integration-tests");
    let idl_compiler = crate::idl_compiler(&root, opts.idl_compiler.clone());
    let all_languages = [
        "c",
        "python",
        "typescript",
        "csharp",
        "cpp",
        "java",
        "rust",
        "json_schema",
        "pydantic",
    ];
    let languages: HashSet<_> = if opts.lang.is_empty() || opts.lang.contains("all") {
        all_languages.iter().map(ToString::to_string).collect()
    } else {
        opts.lang.clone()
    };

    for lang in &languages {
        match lang.as_str() {
            "c" => run_cmake_tests(
                &integration_dir.join("c"),
                &build_dir.join("c"),
                &idl_compiler,
            ),
            "python" | "py" => run_python_tests(&integration_dir, &build_dir, &idl_compiler),
            "typescript" | "ts" => run_typescript_tests(&integration_dir, &idl_compiler),
            "csharp" | "cs" => run_csharp_tests(&integration_dir, &build_dir, &idl_compiler),
            "cpp" | "c++" => run_cmake_tests(
                &integration_dir.join("cpp"),
                &build_dir.join("cpp"),
                &idl_compiler,
            ),
            "pydantic" => run_pydantic_tests(&integration_dir),
            "java" => run_java_tests(&integration_dir, &build_dir, &idl_compiler),
            "json_schema" => run_json_schema_tests(&integration_dir, &build_dir, &idl_compiler),
            "rust" | "rs" => run_rust_tests(&integration_dir, &build_dir, &idl_compiler),
            _ => {
                eprintln!("error: unknown or unsupported language '{lang}'");
                eprintln!(
                    "supported languages: c, python, typescript, csharp, cpp, java, rust, \
                     pydantic, all"
                );
                std::process::exit(1);
            }
        }
    }

    println!("Integration tests passed!");
}

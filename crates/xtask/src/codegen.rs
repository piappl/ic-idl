// Copyright 2025 KONGSBERG
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

use std::path::PathBuf;
use std::process::Command;

/// Run codegen compilation tests
#[derive(ic_cli::Command, Default)]
pub struct Options {
    /// Languages to test (default: all)
    #[option(short, long, arg = "lang")]
    pub lang: Vec<String>,

    /// Number of parallel jobs
    #[option(long, short = 'j', arg = "N")]
    pub jobs: Option<usize>,

    /// Custom corpus directory
    #[option(long, arg = "dir")]
    pub corpus: Option<String>,

    /// Path to Java compiler (javac)
    #[option(long, arg = "path")]
    pub java_compiler: Option<String>,

    /// Path to .NET SDK (dotnet)
    #[option(long, arg = "path")]
    pub dotnet: Option<String>,

    /// Path to Protocol Buffers compiler
    #[option(long, arg = "path")]
    pub protoc: Option<String>,

    /// Path to TypeScript compiler
    #[option(long, arg = "path")]
    pub tsc: Option<String>,

    /// Path to Cargo
    #[option(long, arg = "path")]
    pub cargo: Option<String>,

    /// Path to C compiler
    #[option(long, arg = "path")]
    pub c_compiler: Option<String>,

    /// Path to C++ compiler
    #[option(long, arg = "path")]
    pub cpp_compiler: Option<String>,

    /// Path to ic-idl compiler executable
    #[option(long, arg = "path")]
    pub idl_compiler: Option<String>,

    /// Verbose output
    #[option(short, long)]
    pub verbose: bool,

    /// Treat skipped tests as failures
    #[option(long)]
    pub strict_skip: bool,
}

fn git_root() -> PathBuf {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .unwrap();

    PathBuf::from(std::str::from_utf8(&output.stdout).unwrap().trim())
}

fn lang_to_test_file(lang: &str) -> Option<&'static str> {
    match lang {
        "csharp" | "cs" => Some("tests/test_csharp.py"),
        "java" => Some("tests/test_java.py"),
        "protobuf" => Some("tests/test_protobuf.py"),
        "python" => Some("tests/test_python.py"),
        "typescript" | "ts" => Some("tests/test_typescript.py"),
        "json" => Some("tests/test_json.py"),
        "json-schema" => Some("tests/test_json_schema.py"),
        "xml" => Some("tests/test_xml.py"),
        "idl" => Some("tests/test_idl.py"),
        "rust" | "rs" => Some("tests/test_rust.py"),
        "c" => Some("tests/test_c.py"),
        "cpp" | "c++" => Some("tests/test_cpp.py"),
        "pydantic" | "pd" => Some("tests/test_pydantic.py"),
        _ => None,
    }
}

pub fn run(opts: Options) {
    let workspace_root = git_root();
    let codegen_tests_dir = workspace_root.join("codegen-tests");
    let idl_compiler = crate::idl_compiler(&workspace_root, opts.idl_compiler);

    let corpus = opts.corpus.map_or_else(
        || codegen_tests_dir.join("corpus"),
        |c| std::env::current_dir().unwrap().join(c),
    );

    let test_files: Vec<&str> = if opts.lang.is_empty() || opts.lang.iter().any(|l| l == "all") {
        vec![]
    } else {
        let mut files = Vec::new();
        for lang in &opts.lang {
            if let Some(file) = lang_to_test_file(lang) {
                files.push(file);
            } else {
                eprintln!("error: unknown or unsupported language '{lang}'");
                eprintln!(
                    "supported languages: c, cpp, csharp, java, protobuf, python, rust, \
                     typescript, json, json-schema, xml, idl, pydantic, all"
                );
                std::process::exit(1);
            }
        }
        files
    };

    let jobs = opts
        .jobs
        .map_or_else(|| "auto".to_string(), |n| n.to_string());

    let mut cmd = Command::new("uv");
    cmd.current_dir(&codegen_tests_dir)
        .args(["run", "pytest"])
        .args(&test_files)
        .arg(format!("--idl-compiler={}", idl_compiler.display()))
        .arg(format!("--corpus={}", corpus.display()))
        .arg(format!("-n={jobs}"));

    if let Some(java) = &opts.java_compiler {
        cmd.arg(format!("--java-compiler={java}"));
    }
    if let Some(dotnet) = &opts.dotnet {
        cmd.arg(format!("--dotnet={dotnet}"));
    }
    if let Some(protoc) = &opts.protoc {
        cmd.arg(format!("--protoc={protoc}"));
    }
    if let Some(tsc) = &opts.tsc {
        cmd.arg(format!("--tsc={tsc}"));
    }
    if let Some(cargo) = &opts.cargo {
        cmd.arg(format!("--cargo={cargo}"));
    }
    if let Some(c) = &opts.c_compiler {
        cmd.arg(format!("--c-compiler={c}"));
    }
    if let Some(cpp) = &opts.cpp_compiler {
        cmd.arg(format!("--cpp-compiler={cpp}"));
    }
    if opts.strict_skip {
        cmd.arg("--strict-skip");
    }

    if !opts.verbose {
        cmd.arg("-q");
    }

    let status = match cmd.status() {
        Ok(status) => status,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            eprintln!("error: uv not found. Install it from https://docs.astral.sh/uv/");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("error: failed to run pytest: {e}");
            std::process::exit(1);
        }
    };
    std::process::exit(status.code().unwrap_or(1));
}

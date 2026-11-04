// Copyright 2024 KONGSBERG
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

#![allow(clippy::print_stdout, clippy::print_stderr)]

use std::{backtrace, panic};

use ic_cli::color::Colorize as _;
use ic_cli::{Command, ParseError};
use ic_emit::File;
use ic_idl::util::{self, IgnoreBrokenPipe};
use ic_idl::{CompileDiagnostics, CompileError, Compiler, CompilerOptions, ErrorFormat};
use tracing::{Level, info, info_span};
use tracing_subscriber::fmt;

mod info;
mod parse;
mod unstable;

fn report_error(format: ErrorFormat, msg: &str) {
    if format == ErrorFormat::Json {
        eprintln!(
            "{}",
            ic_diagnostic::json_message(ic_diagnostic::Level::Error, msg),
        );
    } else {
        eprintln!("ic-idl · {} {msg}", "error:".red().bold());
    }
}

fn report_warning(format: ErrorFormat, msg: &str) {
    if format == ErrorFormat::Json {
        eprintln!(
            "{}",
            ic_diagnostic::json_message(ic_diagnostic::Level::Warning, msg),
        );
    } else {
        eprintln!("{} {msg}", "warning:".yellow().bold());
    }
}

fn main() {
    let result = CompilerOptions::command()
        .split_flags(false)
        .align_sections(true)
        .try_parse();

    let result = match result {
        Ok(v) => v,
        Err(ParseError::Help(v)) => {
            println!("{v}");
            return;
        }
        Err(ParseError::Status(v)) => {
            report_error(ErrorFormat::Detailed, &v);
            std::process::exit(1);
        }
    };

    let options = CompilerOptions::from_result(&result);

    if options.error_format == ErrorFormat::Json {
        ic_cli::color::set_color_override(ic_cli::color::ColorMode::Never);
    }

    // Handle special flags
    if options.version {
        println!("{}", info::version());
        return;
    }

    if options.unstable.help {
        unstable::unstable_help();
        return;
    }

    if options.warn.help {
        unstable::warning_help();
        return;
    }

    // Print unknown warnings
    for unknown in &options.warn.unknown_warnings {
        report_warning(
            options.error_format,
            &format!("unknown warning '{}'", unknown.yellow()),
        );
    }

    // Enable tracing
    if let Some(level) = options.unstable.trace.as_deref() {
        init_tracing(level);
    }

    // Expand files
    let files = match util::collect_files(&options.files) {
        Ok(files) => files,
        Err(e) => {
            report_error(options.error_format, &e.to_string());
            std::process::exit(1);
        }
    };

    // Replace the options files with the expanded list
    let mut options = options;
    options.files = files;

    if options.files.is_empty() {
        report_error(options.error_format, "no input files");
        return;
    }

    // Install a panic handler to catch failed asserts.
    panic::set_hook(Box::new(dump_backtrace));

    // Run the compilation pipeline
    try_compile(options);
}

fn try_compile(options: CompilerOptions) {
    info!(files = ?options.files, "starting compilation");

    // Create and run the compiler
    let mut compiler = Compiler::new(options);

    // Handle preprocessor-only mode
    if compiler.options().preprocessor_only {
        run_preprocessor_only(&compiler);
        return;
    }

    // Handle parse-only mode
    if compiler.options().unstable.parse_only {
        run_parse_only(&mut compiler);
        return;
    }

    // Compile to HIR
    let (hir, diagnostics) = match compiler.compile() {
        Ok((hir, diag)) => (hir, diag),
        Err(CompileError::Io(e)) => {
            report_error(compiler.options().error_format, &e.to_string());
            std::process::exit(1);
        }
        Err(CompileError::Diagnostics(diagnostics)) => {
            emit_diagnostics(&compiler, &diagnostics, compiler.options().error_format);
            info!(
                errors = diagnostics.errors.len(),
                warnings = diagnostics.warnings.len(),
                "failed"
            );
            std::process::exit(1);
        }
    };

    // Extract warnings from compile diagnostics
    let warning_count = diagnostics.warnings.len();
    let warnings = diagnostics.warnings;

    // Emit any warnings
    if !warnings.is_empty() {
        let diag = CompileDiagnostics {
            errors: Vec::new(),
            warnings,
            expansion_info: std::collections::HashMap::new(),
        };
        emit_diagnostics(&compiler, &diag, compiler.options().error_format);
    }

    // Dump HIR if requested
    if compiler.options().unstable.hir_dump {
        let tree = ic_hir_tree::emit_tree(&hir, compiler.source_map());
        println!("{tree}");
    }

    // Generate code using backends (they will convert HIR to ptree as needed)
    let generated = match generate_code(compiler.options(), &hir, compiler.source_map()) {
        Ok(files) => files,
        Err(e) => {
            report_error(
                compiler.options().error_format,
                &format!("code generation error: {e}"),
            );
            std::process::exit(1);
        }
    };

    // Handle output
    if compiler.options().list {
        for path in compiler
            .source_map()
            .files()
            .keys()
            .filter(|path| path.is_absolute())
        {
            println!("dep:{}", path.display());
        }

        for f in &generated {
            println!("{f}");
        }
    } else if let Err(e) = write_files(&generated) {
        report_error(
            compiler.options().error_format,
            &format!("failed to write files: {e}"),
        );
        std::process::exit(1);
    }

    info!(
        errors = 0,
        warnings = warning_count,
        outputs = generated.len(),
        "completed"
    );
}

fn run_preprocessor_only(compiler: &Compiler) {
    let proc_args = compiler.proc_args();

    for file in &compiler.options().files {
        match parse::preprocess_only(file, proc_args.clone()) {
            Ok(output) => {
                println!("{output}");
            }
            Err(e) => {
                report_error(compiler.options().error_format, &e.to_string());
                std::process::exit(1);
            }
        }
    }
}

fn run_parse_only(compiler: &mut Compiler) {
    let proc_args = compiler.proc_args();

    for file in compiler.options().files.clone() {
        match parse::from_path(&file, proc_args.clone(), compiler.source_map_mut()) {
            Ok(ast) => {
                if !ast.errors.is_empty() {
                    let formatted = ic_idl::pretty::fmt_errors(
                        &ast.errors,
                        compiler.source_map(),
                        &ast.expansion_info,
                        compiler.options().error_format,
                    );
                    eprintln!("{formatted}");
                    info!(errors = ast.errors.len(), "failed");
                    std::process::exit(1);
                }

                if compiler.options().unstable.ast_dump {
                    println!("{:#?}", ast.tree);
                }
                info!(errors = 0, "completed");
            }
            Err(e) => {
                report_error(compiler.options().error_format, &e.to_string());
                std::process::exit(1);
            }
        }
    }
}

macro_rules! backends {
    ($generated:ident, $options:ident; $( $field:ident, $name:literal => $codegen:expr );* $(;)?) => {
        $(
            if let Some(output_dir) = &$options.codegen.$field {
                let _span = info_span!($name, output_dir = %output_dir.display()).entered();
                let files = invoke_backend(output_dir, || $codegen, $options.purge_dirs)?;
                info!(files = files.len(), "generated");
                $generated.extend(files);
            }
        )*
    };
}

fn generate_code(
    options: &CompilerOptions,
    hir: &ic_hir::ResolvedGraph,
    vfs: &ic_vfs::SourceMap,
) -> Result<Vec<File>, util::Error> {
    let _codegen_span = info_span!("codegen").entered();
    let mut generated = vec![];

    backends! {
        generated, options;
        c_out, "c" => ic_codegen_c::codegen_c(hir, vfs, options.c.clone());
        cpp_out, "cpp" => ic_codegen_cpp::codegen_cpp(hir, vfs, options.cpp.clone());
        csharp_out, "csharp" => ic_codegen_csharp::codegen_csharp(hir, vfs, options.csharp);
        rust_out, "rust" => ic_codegen_rust::codegen_rust(hir, options.rust);
        python_out, "python" => ic_codegen_python::codegen_python(hir, vfs, options.python.clone());
        idl_out, "idl" => ic_codegen_idl::codegen_idl(hir, vfs, options.idl);
        java_out, "java" => ic_codegen_java::codegen_java(hir, options.java.clone());
        json_out, "json" => ic_codegen_json::codegen_json(hir, vfs);
        json_schema_out, "json_schema" => {
            ic_codegen_json_schema::codegen_schema(hir, vfs, options.json_schema.clone())
        };
        xml_out, "xml" => ic_codegen_xml::codegen_xml(hir, vfs);
        proto_out, "proto" => ic_codegen_protobuf::codegen_proto(hir, options.proto.clone());
        pydantic_out, "pydantic" => ic_codegen_pydantic::codegen_pydantic(hir, vfs, &options.pydantic);
        typescript_out, "typescript"  => {
            ic_codegen_typescript::codegen_typescript(hir, options.typescript.clone())
        };
    }

    Ok(generated)
}

fn invoke_backend(
    output_dir: &std::path::Path,
    backend_fn: impl FnOnce() -> Vec<File>,
    purge_dirs: bool,
) -> Result<Vec<File>, util::Error> {
    let dir = std::path::absolute(output_dir)?;
    if purge_dirs {
        util::safe_purge(&dir)?;
        std::fs::create_dir_all(&dir)?;
    }

    let files = backend_fn()
        .into_iter()
        .map(|file| File {
            path: output_dir.join(file.path),
            source: file.source,
        })
        .collect();

    Ok(files)
}

fn write_files(files: &[File]) -> std::io::Result<()> {
    for File { path, source } in files {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }

        tracing::debug!(file = %path.display(), bytes = source.len(), "generated");
        util::write_if_changed(path, source)?;
    }

    Ok(())
}

#[allow(clippy::same_functions_in_if_condition)]
fn emit_diagnostics(compiler: &Compiler, diagnostics: &CompileDiagnostics, format: ErrorFormat) {
    if !diagnostics.warnings.is_empty() {
        let warnings =
            ic_idl::pretty::fmt_warnings(&diagnostics.warnings, compiler.source_map(), format);
        eprint!("{warnings}");

        if !diagnostics.errors.is_empty() && format == ErrorFormat::Detailed {
            eprintln!();
        }
    }

    if !diagnostics.errors.is_empty() {
        let formatted = ic_idl::pretty::fmt_errors(
            &diagnostics.errors,
            compiler.source_map(),
            &diagnostics.expansion_info,
            format,
        );
        eprint!("{formatted}");
        if format == ErrorFormat::Detailed {
            eprintln!();
        }
    } else if !diagnostics.warnings.is_empty() && format == ErrorFormat::Detailed {
        eprintln!();
    }

    let error_plural = if diagnostics.errors.len() > 1 {
        "s"
    } else {
        ""
    };
    let warning_plural = if diagnostics.warnings.len() > 1 {
        "s"
    } else {
        ""
    };

    if !diagnostics.warnings.is_empty() && !diagnostics.errors.is_empty() {
        report_error(
            format,
            &format!(
                "aborting due to {} error{}, {} warning{}",
                diagnostics.errors.len(),
                error_plural,
                diagnostics.warnings.len(),
                warning_plural,
            ),
        );
    } else if !diagnostics.errors.is_empty() {
        report_error(
            format,
            &format!(
                "aborting due to {} error{}",
                diagnostics.errors.len(),
                error_plural,
            ),
        );
    } else if !diagnostics.warnings.is_empty() {
        report_warning(
            format,
            &format!(
                "{} warning{} emitted",
                diagnostics.warnings.len(),
                warning_plural,
            ),
        );
    }
}

fn init_tracing(level: &str) {
    let level = match level {
        "debug" => Level::DEBUG,
        "info" => Level::INFO,
        "warn" => Level::WARN,
        "error" => Level::ERROR,
        _ => Level::TRACE,
    };

    fmt()
        .with_writer(|| IgnoreBrokenPipe(std::io::stderr()))
        .with_timer(fmt::time::uptime())
        .with_span_events(fmt::format::FmtSpan::CLOSE)
        .with_max_level(level)
        .with_ansi(ic_cli::color::has_colors())
        .init();
}

fn dump_backtrace(info: &std::panic::PanicHookInfo) {
    let thread = std::thread::current();
    let thread = thread.name().unwrap_or("unknown");
    let trace = backtrace::Backtrace::force_capture();

    let msg = match info.payload().downcast_ref::<&str>() {
        Some(s) => *s,
        None => info
            .payload()
            .downcast_ref::<String>()
            .map_or("<null>", |s| &**s),
    };

    match info.location() {
        Some(loc) => {
            report_error(
                ErrorFormat::Detailed,
                &format!(
                    "thread '{thread}' panicked at '{msg}', {}:{}",
                    loc.file(),
                    loc.line(),
                ),
            );
        }
        None => {
            report_error(
                ErrorFormat::Detailed,
                &format!("thread '{thread}' panicked at '{msg}'"),
            );
        }
    }

    if trace.status() == backtrace::BacktraceStatus::Captured {
        eprintln!("{trace:#?}");
    }
    eprintln!(
        "This is a compiler bug. Please report it to KONGSBERG <DDS-InterCOM@kda.kongsberg.com>.",
    );
}

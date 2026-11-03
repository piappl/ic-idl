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

#![allow(unused, clippy::doc_markdown)]

use std::collections::HashMap;
use std::path::PathBuf;

use ic_cli::Command;
use ic_cli::color::Colorize as _;
use ic_cli::convert::{self, ConvertError};
use ic_lint::{Category, Level, LintConfig};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ErrorFormat {
    #[default]
    Detailed,
    Short,
    Json,
}

impl convert::Convert for ErrorFormat {
    fn from_result(input: &[String]) -> convert::Result<Self> {
        match input.last().map(String::as_str) {
            Some("detailed") | None => Ok(Self::Detailed),
            Some("short") => Ok(Self::Short),
            Some("json") => Ok(Self::Json),
            Some(other) => Err(ConvertError::InvalidValue(format!(
                "invalid error format '{}', expected 'detailed', 'short' or 'json'",
                other.yellow(),
            ))),
        }
    }
}

#[derive(Command, Debug)]
pub struct Warnings {
    /// Enable all warnings
    #[option(long)]
    all: bool,

    /// Suspicious annotation usage such as conflicting annotations
    #[option(long)]
    annotation: bool,

    /// Non-standard language extensions
    #[option(long)]
    extensions: bool,

    /// Nitpicky style and quality warnings
    #[option(long)]
    pedantic: bool,

    /// Preprocessor-related warnings
    #[option(long)]
    preprocessor: bool,

    /// Unsupported language constructs
    #[option(long)]
    unsupported: bool,

    /// Upgrade warnings to errors
    #[option(long)]
    error: bool,

    /// Show help for warning options
    pub help: bool,

    specific_lints: HashMap<String, bool>,

    error_lints: HashMap<String, bool>,

    /// Unknown warnings that were encountered during parsing
    pub unknown_warnings: Vec<String>,
}

impl Warnings {
    /// Build a LintConfig from the warning flags.
    pub fn to_lint_config(&self) -> LintConfig {
        let mut config = LintConfig::new();

        // Start with most categories disabled by default
        config.set_category_level(Category::Annotation, Level::Disabled);
        config.set_category_level(Category::Deprecated, Level::Disabled);
        config.set_category_level(Category::Extensions, Level::Disabled);
        config.set_category_level(Category::Pedantic, Level::Disabled);
        config.set_category_level(Category::Unsupported, Level::Disabled);

        if !self.preprocessor {
            config.set_category_level(Category::Preprocessor, Level::Disabled);
        }

        // Enable specific categories if requested
        if self.annotation {
            config.set_category_level(Category::Annotation, Level::Warning);
        }

        if self.extensions {
            config.set_category_level(Category::Extensions, Level::Warning);
        }

        if self.pedantic {
            config.set_category_level(Category::Pedantic, Level::Warning);
        }

        if self.unsupported {
            config.set_category_level(Category::Unsupported, Level::Warning);
        }

        // Apply specific lint settings
        for (lint_name, enabled) in &self.specific_lints {
            let level = if *enabled {
                Level::Warning
            } else {
                Level::Disabled
            };
            config.set_lint_level(lint_name.as_str(), level);
        }

        // Apply lint or category error settings
        for name in self.error_lints.keys() {
            if let Some(category) = parse_category(name) {
                config.set_category_level(category, Level::Error);
            } else {
                config.set_lint_level(name.as_str(), Level::Error);
            }
        }

        // If -Werror is set, upgrade all enabled warnings to errors
        if self.error {
            if config.category_levels.get(&Category::Annotation) == Some(&Level::Warning) {
                config.set_category_level(Category::Annotation, Level::Error);
            }
            if config.category_levels.get(&Category::Extensions) == Some(&Level::Warning) {
                config.set_category_level(Category::Extensions, Level::Error);
            }
            if config.category_levels.get(&Category::Pedantic) == Some(&Level::Warning) {
                config.set_category_level(Category::Pedantic, Level::Error);
            }
            if config.category_levels.get(&Category::Preprocessor) == Some(&Level::Warning) {
                config.set_category_level(Category::Preprocessor, Level::Error);
            }
            if config.category_levels.get(&Category::Unsupported) == Some(&Level::Warning) {
                config.set_category_level(Category::Unsupported, Level::Error);
            }
            if config.category_levels.get(&Category::Deprecated) == Some(&Level::Warning) {
                config.set_category_level(Category::Deprecated, Level::Error);
            }

            // Upgrade individual lint levels
            let lint_names: Vec<_> = config.lint_levels.keys().copied().collect();
            for lint_name in lint_names {
                if config.lint_levels.get(lint_name) == Some(&Level::Warning) {
                    config.set_lint_level(lint_name, Level::Error);
                }
            }
        }

        config
    }

    fn clear_category_lints(&mut self, category: Category) {
        for lint in ic_lint::all_lints() {
            if lint.category == category {
                self.specific_lints.remove(lint.name);
            }
        }
    }
}

/// Generic IDL code generator
#[derive(Command, Debug, Default)]
#[must_use]
pub struct Options {
    /// Only run the preprocessor
    #[option(short = 'E', long)]
    pub preprocessor_only: bool,

    /// Add directory to include search paths
    #[option(short = 'I', long, arg = "dir")]
    pub include: Vec<String>,

    /// Define preprocessor directive <def> to <val>
    #[option(short = 'D', long, arg = "def=val")]
    pub define: Vec<String>,

    /// Output list of files to be generated
    #[option(short, long)]
    pub list: bool,

    /// Erase output directories before emitting code
    #[option(long)]
    pub purge_dirs: bool,

    /// Ignore Doxygen-style comments
    #[option(long)]
    pub ignore_comments: bool,

    /// Error output format: detailed, short or json
    #[option(long, arg = "fmt")]
    pub error_format: ErrorFormat,

    /// Enable a warning, see `-W help` for details
    #[option(short = 'W', arg = "lint")]
    pub warn: Warnings,

    /// Unstable flags, see `-Z help` for details
    #[option(short = 'Z', arg = "flag")]
    pub unstable: Unstable,

    /// Display version information
    #[option(short = 'V', long)]
    pub version: bool,

    #[option(positional)]
    pub files: Vec<PathBuf>,

    #[option(section = "c options")]
    pub c: ic_codegen_c::COptions,

    #[option(section = "c++ options")]
    pub cpp: ic_codegen_cpp::CppOptions,

    #[option(section = "c# options")]
    pub csharp: ic_codegen_csharp::CSharpOptions,

    #[option(section = "rust options")]
    pub rust: ic_codegen_rust::RustOptions,

    #[option(section = "python options")]
    pub python: ic_codegen_python::PythonOptions,

    #[option(section = "pydantic options")]
    pub pydantic: ic_codegen_pydantic::PydanticOptions,

    #[option(section = "idl options")]
    pub idl: ic_codegen_idl::IdlOptions,

    #[option(section = "proto options")]
    pub proto: ic_codegen_protobuf::ProtoOptions,

    #[option(section = "java options")]
    pub java: ic_codegen_java::JavaOptions,

    #[option(section = "json schema options")]
    pub json_schema: ic_codegen_json_schema::JsonSchemaOptions,

    #[option(section = "typescript options")]
    pub typescript: ic_codegen_typescript::TypeScriptOptions,

    #[option(section = "backends")]
    pub codegen: CodegenOptions,
}

#[derive(Command, Debug, Default)]
#[allow(clippy::struct_field_names)]
pub struct CodegenOptions {
    /// Generate C files in <dir>
    #[option(long, arg = "dir")]
    pub c_out: Option<PathBuf>,

    /// Generate C++ files in <dir>
    #[option(long, arg = "dir")]
    pub cpp_out: Option<PathBuf>,

    /// Generate C# files in <dir>
    #[option(long, arg = "dir")]
    pub csharp_out: Option<PathBuf>,

    /// Generate IDL files in <dir>
    #[option(long, arg = "dir")]
    pub idl_out: Option<PathBuf>,

    /// Generate Java files in <dir>
    #[option(long, arg = "dir")]
    pub java_out: Option<PathBuf>,

    /// Generate JSON files in <dir>
    #[option(long, arg = "dir")]
    pub json_out: Option<PathBuf>,

    /// Generate JSON Schema files in <dir>
    #[option(long, arg = "dir")]
    pub json_schema_out: Option<PathBuf>,

    /// Generate Protobuf files in <dir>
    #[option(long, arg = "dir")]
    pub proto_out: Option<PathBuf>,

    /// Generate Pydantic files in <dir>
    #[option(long, arg = "dir")]
    pub pydantic_out: Option<PathBuf>,

    /// Generate Python files in <dir>
    #[option(long, arg = "dir")]
    pub python_out: Option<PathBuf>,

    /// Generate Rust files in <dir>
    #[option(long, arg = "dir")]
    pub rust_out: Option<PathBuf>,

    /// Generate TypeScript files in <dir>
    #[option(long, arg = "dir")]
    pub typescript_out: Option<PathBuf>,

    /// Generate XML files in <dir>
    #[option(long, arg = "dir")]
    pub xml_out: Option<PathBuf>,
}

#[derive(Command, Debug, Default)]
pub struct Unstable {
    /// Dump out the AST exactly as it was parsed
    #[option(long)]
    pub ast_dump: bool,

    /// Dump out the type-resolved IR
    #[option(long)]
    pub hir_dump: bool,

    /// Only parse, skip type checking and code generation
    #[option(long)]
    pub parse_only: bool,

    /// Enable tracing output (trace, debug, info, warn, error)
    #[option(long, arg = "level")]
    pub trace: Option<String>,

    /// Show help for unstable options
    pub help: bool,
}

impl convert::Convert for Unstable {
    fn from_result(input: &[String]) -> convert::Result<Self> {
        let mut this = Self::default();
        for arg in input {
            // Check for trace=level or just trace
            if let Some(level) = arg.strip_prefix("trace=") {
                match level {
                    "trace" | "debug" | "info" | "warn" | "error" => {
                        this.trace = Some(level.to_string());
                    }
                    _ => {
                        return Err(ConvertError::InvalidValue(format!(
                            "invalid trace level '{}', expected one of: trace, debug, info, warn, \
                             error",
                            level.yellow(),
                        )));
                    }
                }
                continue;
            }

            match arg.as_str() {
                "ast-dump" => this.ast_dump = true,
                "hir-dump" => this.hir_dump = true,
                "parse-only" => this.parse_only = true,
                "trace" => this.trace = Some("trace".to_string()),
                "help" => {
                    this.help = true;
                    return Ok(this);
                }
                _ => {
                    return Err(ConvertError::InvalidValue(format!(
                        "unknown unstable option '{}'",
                        format!("-Z{arg}").yellow(),
                    )));
                }
            }
        }
        Ok(this)
    }
}

const DEFAULT_LINTS: &[&str] = &["complex-key", "omitted-in", "prefer-enum-name"];

/// Try to parse a category name from a string.
fn parse_category(name: &str) -> Option<Category> {
    match name {
        "annotation" => Some(Category::Annotation),
        "extensions" => Some(Category::Extensions),
        "pedantic" => Some(Category::Pedantic),
        "preprocessor" => Some(Category::Preprocessor),
        "unsupported" => Some(Category::Unsupported),
        _ => None,
    }
}

impl Default for Warnings {
    fn default() -> Self {
        Self {
            all: false,
            annotation: true,
            extensions: false,
            pedantic: false,
            preprocessor: true,
            unsupported: true,
            error: false,
            help: false,
            specific_lints: DEFAULT_LINTS
                .iter()
                .map(|name| ((*name).to_string(), true))
                .collect(),
            error_lints: HashMap::new(),
            unknown_warnings: Vec::new(),
        }
    }
}

impl convert::Convert for Warnings {
    fn from_result(input: &[String]) -> convert::Result<Self> {
        let mut warnings = Self::default();
        let known_lints = ic_lint::all_lint_names();

        for arg in input {
            // Handle error=lint-name or error=category syntax
            if let Some(name) = arg.strip_prefix("error=") {
                if let Some(category) = parse_category(name) {
                    warnings.clear_category_lints(category);
                }

                if known_lints.contains(&name) || parse_category(name).is_some() {
                    warnings.error_lints.insert(name.to_string(), true);
                } else {
                    warnings.unknown_warnings.push(format!("-Werror={name}"));
                }
                continue;
            }

            let (arg, enabled) = if let Some(arg) = arg.strip_prefix("no-") {
                (arg, false)
            } else {
                (arg.as_str(), true)
            };

            if let Some(category) = parse_category(arg) {
                warnings.clear_category_lints(category);
            }

            match arg {
                "all" => {
                    warnings.specific_lints.clear();
                    warnings.all = enabled;
                    warnings.annotation = enabled;
                    warnings.extensions = enabled;
                    warnings.pedantic = enabled;
                    warnings.preprocessor = enabled;
                    warnings.unsupported = enabled;
                }
                "annotation" => warnings.annotation = enabled,
                "extensions" => warnings.extensions = enabled,
                "pedantic" => warnings.pedantic = enabled,
                "preprocessor" => warnings.preprocessor = enabled,
                "unsupported" => warnings.unsupported = enabled,
                "error" => warnings.error = enabled,
                "help" => {
                    warnings.help = true;
                    return Ok(warnings);
                }
                _ => {
                    // Check if it's a specific lint name
                    if known_lints.contains(&arg) {
                        warnings.specific_lints.insert(arg.to_string(), enabled);
                    } else {
                        warnings.unknown_warnings.push(format!("-W{arg}"));
                    }
                }
            }
        }

        Ok(warnings)
    }
}

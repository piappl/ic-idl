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

mod codegen;
mod writer;

use ic_cli::Command;
use ic_cli::convert::{Convert, ConvertError, Result};
use ic_emit::File;
use ic_hir::ResolvedGraph;
use ic_vfs::SourceMap;

use crate::codegen::{generate, module_stem};

#[derive(Debug, Default, Clone)]
pub struct FullyQualifiedName(pub String);

impl std::ops::Deref for FullyQualifiedName {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Convert for FullyQualifiedName {
    fn from_result(input: &[String]) -> Result<Self> {
        let s = input.last().unwrap();
        if !s.contains('.') {
            return Err(ConvertError::InvalidValue(format!(
                "'{s}' must be a fully qualified class name (e.g. 'my.package.MyClass')"
            )));
        }
        Ok(FullyQualifiedName(s.clone()))
    }
}

#[derive(Debug, Default, Clone)]
pub struct FilenameStem(pub String);

impl std::ops::Deref for FilenameStem {
    type Target = str;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Convert for FilenameStem {
    fn from_result(input: &[String]) -> Result<Self> {
        let s = input.last().unwrap();
        let Some(stem) = module_stem(s) else {
            return Err(ConvertError::InvalidValue(format!(
                "'{s}' must be a valid Python module name: letters, digits and underscores only"
            )));
        };
        Ok(FilenameStem(stem))
    }
}

#[derive(Command, Debug, Default, Clone)]
pub struct PydanticOptions {
    /// Base class for generated pydantic classes, defaults to `pydantic.BaseModel`.
    #[option(long, arg = "class")]
    pub model_base_class: Option<FullyQualifiedName>,

    /// Custom base class for generated enums.
    #[option(long, arg = "class")]
    pub enum_base_class: Option<FullyQualifiedName>,

    /// Suppress native type base for enums (e.g. `str`, `int`).
    #[option(long)]
    pub enum_no_primitive_base: bool,

    /// Default value strategy for enums: "literal-str" (default), "literal-int", "auto-str", "auto-int".
    #[option(long, arg = "strategy")]
    pub enum_defaults: Option<String>,

    /// Override enum serialization schema ("name", "value", "none"). Default: "value" for enums with declared values, "none" with a custom enum base class, otherwise "name".
    #[option(long, arg = "mode")]
    pub enum_schema_override: Option<String>,

    /// Custom stem for the generated `_cidl_*.py` files: letters, digits and underscores only.
    #[option(long, arg = "stem")]
    pub filename_stem: Option<FilenameStem>,

    /// Disable generation of implicit default values for required fields.
    #[option(long)]
    pub no_implicit_defaults: bool,
}

#[must_use]
pub fn codegen_pydantic(
    hir: &ResolvedGraph,
    source_map: &SourceMap,
    options: &PydanticOptions,
) -> Vec<File> {
    generate(hir, source_map, options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filename_stem_accepts_module_names() {
        for value in ["model", "ok_stem", "_private", "Camel2", "A"] {
            let stem = FilenameStem::from_result(&[value.to_string()])
                .unwrap_or_else(|_| panic!("{value}"));
            assert_eq!(&*stem, value);
        }
    }

    #[test]
    fn test_filename_stem_rejects_unimportable_names() {
        for value in [
            "", ".", "..", "../x", "a/b", "a\\b", "/abs", "a-b", "a.b", "9lead", "a b",
        ] {
            let Err(ConvertError::InvalidValue(msg)) =
                FilenameStem::from_result(&[value.to_string()])
            else {
                panic!("{value} must be rejected");
            };
            assert!(msg.starts_with('\''), "{msg}");
        }
    }
}

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

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;
use std::path::{Path, PathBuf};

use ic_cli::color::Colorize;
use ic_emit::{File, case};
use ic_hir::ResolvedGraph;
use ic_hir::hir::{Def, DefId, DefKind};
use ic_vfs::{Include, SourceMap};

use crate::PydanticOptions;
use crate::writer::{PydanticWriter, def_is_dead};

pub(crate) struct PydanticGen<'a> {
    hir: &'a ResolvedGraph,
    options: &'a PydanticOptions,
    idl_stem: String,
    module_paths: BTreeMap<DefId, String>,
    dropped_kinds: BTreeMap<&'static str, usize>,
}

impl<'a> PydanticGen<'a> {
    fn new(hir: &'a ResolvedGraph, options: &'a PydanticOptions, idl_stem: String) -> Self {
        Self {
            hir,
            options,
            idl_stem,
            module_paths: BTreeMap::new(),
            dropped_kinds: BTreeMap::new(),
        }
    }

    fn generate(&mut self) -> Vec<File> {
        let mut module_defs: BTreeMap<PathBuf, Vec<DefId>> = BTreeMap::new();
        self.collect_module_defs(&mut module_defs);
        self.report_dropped_kinds();

        let mut files = Vec::new();
        let mut module_structure: BTreeMap<PathBuf, BTreeSet<String>> = BTreeMap::new();
        let def_filename = format!("_cidl_{}.py", self.idl_stem);
        let mut cidl_helper_needed = false;

        let mut all_init_paths = BTreeSet::new();

        for (path, def_ids) in &module_defs {
            all_init_paths.insert(path.clone());

            let file_module_path = path
                .to_str()
                .unwrap()
                .replace(std::path::MAIN_SEPARATOR, ".");
            let namespace = path
                .to_str()
                .unwrap()
                .replace(std::path::MAIN_SEPARATOR, "/");

            let mut writer = PydanticWriter::new(
                self.hir,
                &self.module_paths,
                &file_module_path,
                namespace,
                self.options,
                &self.idl_stem,
            );
            let source = writer.render_module(def_ids);

            if writer.uses_cidl_helper() {
                cidl_helper_needed = true;
            }

            let def_path = path.join(&def_filename);
            files.push(File {
                path: def_path,
                source,
            });

            let mut p = path.as_path();
            while let Some(parent) = p.parent() {
                if let Some(component) = p.file_name().and_then(|s| s.to_str())
                    && !component.is_empty()
                {
                    module_structure
                        .entry(parent.to_path_buf())
                        .or_default()
                        .insert(component.to_owned());
                    all_init_paths.insert(parent.to_path_buf());
                }
                p = parent;
                if parent.as_os_str().is_empty() {
                    break;
                }
            }
        }

        for path in all_init_paths {
            let defs = module_defs.get(&path);
            let submodules = module_structure.get(&path);
            files.push(self.generate_init_file(&path, defs, submodules));
        }

        if cidl_helper_needed {
            files.push(Self::generate_cidl_helper_file());
        }

        files
    }

    /// Traverses the HIR to collect all renderable definitions and map them to their
    /// corresponding Python module paths. It populates `self.module_paths` with the
    /// fully-qualified module path for each definition.
    fn collect_module_defs(&mut self, module_defs: &mut BTreeMap<PathBuf, Vec<DefId>>) {
        for &def_id in &self.hir.order {
            let def = self.hir.context.definitions.get(def_id);
            if def.parent.is_none() {
                match &def.kind {
                    DefKind::Module(_) => {
                        self.collect_defs_recursive(module_defs, &mut PathBuf::new(), "", def_id);
                    }
                    DefKind::Struct(_)
                    | DefKind::Union(_)
                    | DefKind::Enum(_)
                    | DefKind::Alias(_)
                    | DefKind::Const(_) => {
                        module_defs.entry(PathBuf::new()).or_default().push(def_id);
                        self.module_paths
                            .insert(def_id, format!("_cidl_{}", self.idl_stem));
                    }
                    DefKind::Valuetype(_) | DefKind::Except(_) | DefKind::Bitmask(_) => {
                        // The entry and the module path stay so module skeletons and
                        // reference spellings match the unfiltered layout.
                        module_defs.entry(PathBuf::new()).or_default();
                        self.module_paths
                            .insert(def_id, format!("_cidl_{}", self.idl_stem));
                        self.count_dropped(def);
                    }
                    _ => {}
                }
            }
        }
    }

    fn collect_defs_recursive(
        &mut self,
        module_defs: &mut BTreeMap<PathBuf, Vec<DefId>>,
        path: &mut PathBuf,
        module_path: &str,
        module_def_id: DefId,
    ) {
        let module_def = self.hir.context.definitions.get(module_def_id);
        if let DefKind::Module(module) = &module_def.kind {
            let name = PydanticWriter::escape_python_keyword(&module_def.ident.name);
            path.push(&name);
            let new_module_path = if module_path.is_empty() {
                name.clone()
            } else {
                format!("{module_path}.{name}")
            };
            self.module_paths
                .insert(module_def_id, new_module_path.clone());

            let mut has_renderable_children = false;
            for &child_id in &module.definitions {
                let child_def = self.hir.context.definitions.get(child_id);
                match &child_def.kind {
                    DefKind::Struct(_)
                    | DefKind::Union(_)
                    | DefKind::Enum(_)
                    | DefKind::Alias(_)
                    | DefKind::Const(_) => {
                        self.module_paths.insert(child_id, new_module_path.clone());
                        module_defs.entry(path.clone()).or_default().push(child_id);
                        has_renderable_children = true;
                    }
                    DefKind::Valuetype(_) | DefKind::Except(_) | DefKind::Bitmask(_) => {
                        self.module_paths.insert(child_id, new_module_path.clone());
                        module_defs.entry(path.clone()).or_default();
                        self.count_dropped(child_def);
                    }
                    DefKind::Module(_) => {
                        self.collect_defs_recursive(module_defs, path, &new_module_path, child_id);
                    }
                    DefKind::Interface(interface) => {
                        for &child_id in &interface.definitions {
                            let child_def = self.hir.context.definitions.get(child_id);
                            match &child_def.kind {
                                DefKind::Struct(_)
                                | DefKind::Union(_)
                                | DefKind::Enum(_)
                                | DefKind::Alias(_)
                                | DefKind::Const(_) => {
                                    self.module_paths.insert(child_id, new_module_path.clone());
                                    module_defs.entry(path.clone()).or_default().push(child_id);
                                }
                                DefKind::Valuetype(_)
                                | DefKind::Except(_)
                                | DefKind::Bitmask(_) => {
                                    self.module_paths.insert(child_id, new_module_path.clone());
                                    module_defs.entry(path.clone()).or_default();
                                    self.count_dropped(child_def);
                                }
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
            }

            if has_renderable_children {
                module_defs.entry(path.clone()).or_default();
            }

            path.pop();
        }
    }

    fn count_dropped(&mut self, def: &Def) {
        let kind = match &def.kind {
            DefKind::Valuetype(_) => "valuetype",
            DefKind::Except(_) => "exception",
            DefKind::Bitmask(_) => "bitmask",
            _ => return,
        };
        *self.dropped_kinds.entry(kind).or_default() += 1;
    }

    #[allow(clippy::print_stderr)]
    fn report_dropped_kinds(&self) {
        for (kind, count) in &self.dropped_kinds {
            eprintln!(
                "{}: pydantic: {} {} definition(s) are not rendered by this backend and were \
                 dropped",
                "warning".yellow().bold(),
                count,
                kind,
            );
        }
    }

    fn generate_init_file(
        &self,
        path: &Path,
        defs: Option<&Vec<DefId>>,
        submodules: Option<&BTreeSet<String>>,
    ) -> File {
        let mut source = String::new();

        let mut exported_names = Vec::new();
        if let Some(def_ids) = defs {
            let mut sorted_def_ids = def_ids.clone();
            sorted_def_ids.sort_by_key(|&id| {
                let def = self.hir.context.definitions.get(id);
                match &def.kind {
                    DefKind::Const(_) => 0,
                    DefKind::Enum(_) => 1,
                    DefKind::Struct(_) | DefKind::Union(_) => 2,
                    DefKind::Alias(_) => 3,
                    _ => 4,
                }
            });

            for &def_id in &sorted_def_ids {
                let def = self.hir.context.definitions.get(def_id);
                match &def.kind {
                    DefKind::Alias(_) if def_is_dead(self.hir, def_id) => {}
                    DefKind::Struct(_) | DefKind::Enum(_) | DefKind::Alias(_) => {
                        exported_names.push(PydanticWriter::escape_python_keyword(&def.ident.name));
                    }
                    DefKind::Union(union_ty) => {
                        exported_names.push(PydanticWriter::escape_python_keyword(&def.ident.name));
                        for variant in &union_ty.variants {
                            if variant.ident.name != "_implicit_default" {
                                exported_names.push(format!(
                                    "{}{}",
                                    case::pascal(&def.ident.name),
                                    case::pascal(&variant.ident.name)
                                ));
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        if !exported_names.is_empty() {
            writeln!(source, "from ._cidl_{} import (", self.idl_stem).unwrap();
            for name in &exported_names {
                writeln!(source, "    {name},").unwrap();
            }
            writeln!(source, ")").unwrap();
        }

        if let Some(subs) = submodules.filter(|s| !s.is_empty()) {
            if !source.is_empty() {
                source.push('\n');
            }
            writeln!(source, "import importlib as _importlib_").unwrap();
            writeln!(
                source,
                "from typing import TYPE_CHECKING as _TYPE_CHECKING_"
            )
            .unwrap();
            source.push('\n');

            writeln!(source, "__all__ = [").unwrap();
            for name in &exported_names {
                writeln!(source, "    \"{name}\",").unwrap();
            }

            let mut sorted_subs: Vec<&String> = subs.iter().collect();
            sorted_subs.sort();

            for sub in &sorted_subs {
                writeln!(source, "    \"{sub}\",").unwrap();
            }
            writeln!(source, "]").unwrap();
            source.push('\n');

            writeln!(source, "if _TYPE_CHECKING_:").unwrap();
            for sub in &sorted_subs {
                writeln!(source, "    from . import {sub}").unwrap();
            }
            source.push('\n');
            source.push('\n');

            writeln!(source, "def __getattr__(name: str):").unwrap();
            writeln!(source, "    if name in __all__:").unwrap();
            writeln!(
                source,
                "        module = _importlib_.import_module(f\".{{name}}\", __package__)"
            )
            .unwrap();
            writeln!(source, "        globals()[name] = module").unwrap();
            writeln!(source, "        return module").unwrap();
            writeln!(
                source,
                "    raise AttributeError(f\"module {{__name__!r}} has no attribute {{name!r}}\")"
            )
            .unwrap();
        } else if !exported_names.is_empty() {
            writeln!(source, "__all__ = [").unwrap();
            for name in &exported_names {
                writeln!(source, "    \"{name}\",").unwrap();
            }
            writeln!(source, "]").unwrap();
        }

        File {
            path: path.join("__init__.py"),
            source,
        }
    }

    fn generate_cidl_helper_file() -> File {
        const CIDL_HELPER: &str = r#"from typing import Any

from pydantic import GetCoreSchemaHandler
from pydantic_core import CoreSchema, core_schema


def cidl_enum_schema(serialize_by_name: bool = True):
    def decorator(cls):
        def __get_pydantic_core_schema__(
            cls_obj, source_type: Any, handler: GetCoreSchemaHandler
        ) -> CoreSchema:
            original_schema = handler(source_type)

            def validate_serialize_by_name(v: Any) -> Any:
                if isinstance(v, str):
                    try:
                        return cls_obj[v]
                    except KeyError:
                        raise ValueError(f"Invalid name '{v}' for {cls_obj.__name__}")
                return v

            input_schema = core_schema.no_info_before_validator_function(
                validate_serialize_by_name,
                original_schema
            )

            if serialize_by_name:
                return core_schema.json_or_python_schema(
                    json_schema=input_schema,
                    python_schema=input_schema,
                    serialization=core_schema.plain_serializer_function_ser_schema(
                        lambda x: x.name
                    ),
                )
            return input_schema
        cls.__get_pydantic_core_schema__ = classmethod(__get_pydantic_core_schema__)
        return cls
    return decorator
"#;
        File {
            path: PathBuf::from("_cidl.py"),
            source: CIDL_HELPER.to_string(),
        }
    }
}

/// Guards every stem that names a generated module. The emitted `__init__.py` re-exports through
/// `_cidl_<stem>.py`, so a stem with a separator, a dot or a dash names a file that Python cannot
/// import. The stem names a file inside the module directories, not the directory itself.
#[must_use]
pub(crate) fn module_stem(value: &str) -> Option<String> {
    let valid = !value.is_empty()
        && value.starts_with(|c: char| c.is_ascii_alphabetic() || c == '_')
        && value.chars().all(|c| c.is_ascii_alphanumeric() || c == '_');
    valid.then(|| value.to_owned())
}

/// The stem of the generated `_cidl_<stem>.py` module, taken from the root module of the
/// graph so it does not depend on which file the compiler opened first.
fn root_module_stem(hir: &ResolvedGraph) -> Option<String> {
    hir.order.iter().find_map(|&def_id| {
        let def = hir.context.definitions.get(def_id);
        (def.parent.is_none() && matches!(def.kind, DefKind::Module(_)))
            .then(|| def.ident.name.clone())
    })
}

fn static_include_stem(source_map: &SourceMap) -> Option<String> {
    source_map
        .files()
        .values()
        .find_map(|&id| {
            let file_info = source_map.file_info(id);
            if file_info.kind == Include::Static {
                file_info.path.file_stem().and_then(|s| s.to_str())
            } else {
                None
            }
        })
        .map(str::to_owned)
}

pub(crate) fn generate(
    hir: &ResolvedGraph,
    source_map: &SourceMap,
    options: &PydanticOptions,
) -> Vec<File> {
    let idl_stem = options
        .filename_stem
        .as_deref()
        .map(str::to_owned)
        .or_else(|| root_module_stem(hir).and_then(|stem| module_stem(&stem)))
        .or_else(|| static_include_stem(source_map).and_then(|stem| module_stem(&stem)))
        .unwrap_or_else(|| "model".to_string());
    PydanticGen::new(hir, options, idl_stem).generate()
}

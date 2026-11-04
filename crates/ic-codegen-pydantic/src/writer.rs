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
use std::iter::zip;

use ic_emit::case;
use ic_emit::printer::PrettyPrinter;
use ic_hir::ResolvedGraph;
use ic_hir::hir::{
    AliasTy, Ann, ConstTy, Def, DefFlags, DefId, DefKind, EnumTy, Member, Numeric, PrimitiveTy,
    StructTy, Ty, TyKind, UnionTy, Variant,
};
use ic_hir_analysis::annotation::builtin_annotation;

use crate::PydanticOptions;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum ImportCategory {
    StdLib,
    ThirdParty,
    Local,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Import {
    category: ImportCategory,
    module: String,
    name: Option<String>,
    alias: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Axis {
    Numeric,
    Length,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EnumValueMode {
    Declared,
    LiteralInt,
    LiteralStr,
    AutoStr,
    AutoInt,
}

struct ImportManager<'a> {
    options: &'a PydanticOptions,
    current_module_path: &'a str,
    own_module_path: String,
    imports: BTreeSet<Import>,
}

impl<'a> ImportManager<'a> {
    fn new(options: &'a PydanticOptions, current_module_path: &'a str, idl_stem: &'a str) -> Self {
        // File-scope defs are rendered into the `_cidl_<stem>` module of the root package, so that
        // module and not the empty package path is what counts as same-module.
        let own_module_path = if current_module_path.is_empty() {
            format!("_cidl_{idl_stem}")
        } else {
            current_module_path.to_owned()
        };
        Self {
            options,
            current_module_path,
            own_module_path,
            imports: BTreeSet::new(),
        }
    }

    fn add_import(&mut self, module: &str, name: &str, alias: &str) {
        if self.is_local_import(module) {
            self.add_local_import(module);
        } else {
            self.add_external_import(module, Some(name), Some(alias));
        }
    }

    fn add_module_import(&mut self, module: &str, alias: &str) {
        if self.is_local_import(module) {
            self.add_local_import(module);
        } else {
            self.add_external_import(module, None, Some(alias));
        }
    }

    fn add_adt_import(&mut self, module: &str) {
        self.add_local_import(module);
    }

    fn add_external_import(&mut self, module: &str, name: Option<&str>, alias: Option<&str>) {
        let category = if Self::is_std_import(module) {
            ImportCategory::StdLib
        } else {
            ImportCategory::ThirdParty
        };
        self.imports.insert(Import {
            category,
            module: module.to_string(),
            name: name.map(str::to_string),
            alias: alias.map(str::to_string),
        });
    }

    fn add_local_import(&mut self, module: &str) {
        if module == self.own_module_path {
            return;
        }

        let alias = format!("{}_", module.replace('.', "_"));

        let (parent, name) = if let Some(idx) = module.rfind('.') {
            (&module[..idx], &module[idx + 1..])
        } else {
            ("", module)
        };

        let current_parts: Vec<_> = if self.current_module_path.is_empty() {
            Vec::new()
        } else {
            self.current_module_path.split('.').collect()
        };
        let parent_parts: Vec<_> = if parent.is_empty() {
            Vec::new()
        } else {
            parent.split('.').collect()
        };

        let common_prefix_len = zip(current_parts.iter(), parent_parts.iter())
            .take_while(|(a, b)| a == b)
            .count();

        let up_traversals = current_parts.len() - common_prefix_len;
        let down_path_parts = &parent_parts[common_prefix_len..];

        let dots = ".".repeat(up_traversals + 1);
        let from_module = if dots.is_empty() {
            down_path_parts.join(".")
        } else {
            format!("{}{}", dots, down_path_parts.join("."))
        };

        self.imports.insert(Import {
            category: ImportCategory::Local,
            module: from_module,
            name: Some(name.to_string()),
            alias: Some(alias),
        });
    }

    fn add_helper_import(&mut self) {
        let module = "_cidl";
        let current_parts: Vec<_> = self.current_module_path.split('.').collect();
        let from_module = if self.current_module_path.is_empty() {
            // File-scope defs render beside the helper at the generated package root.
            format!(".{module}")
        } else {
            let up_traversals = current_parts.len();
            let dots = ".".repeat(up_traversals + 1);
            format!("{dots}{module}")
        };

        self.imports.insert(Import {
            category: ImportCategory::Local,
            module: from_module,
            name: Some("cidl_enum_schema".to_string()),
            alias: Some("_cidl_enum_schema_".to_string()),
        });
    }

    fn is_local_import(&self, module: &str) -> bool {
        !(Self::is_std_import(module)
            || Self::is_third_party_import(module)
            || self.is_custom_base_import(module))
    }

    fn is_custom_base_import(&self, module: &str) -> bool {
        self.options
            .model_base_class
            .as_deref()
            .is_some_and(|c| c.starts_with(module))
            || self
                .options
                .enum_base_class
                .as_deref()
                .is_some_and(|c| c.starts_with(module))
    }

    fn is_std_import(module: &str) -> bool {
        module == "enum" || module == "typing" || module == "re"
    }

    fn is_third_party_import(module: &str) -> bool {
        module.starts_with("pydantic")
    }

    fn uses_cidl_helper(&self) -> bool {
        self.imports.iter().any(|import| {
            import.name.as_deref() == Some("cidl_enum_schema") || import.module.ends_with("_cidl")
        })
    }
}

type ImportsByCategory =
    BTreeMap<ImportCategory, BTreeMap<String, BTreeSet<(Option<String>, Option<String>)>>>;

impl ImportManager<'_> {
    fn render(&self, printer: &mut PrettyPrinter) {
        let mut imports_by_category: ImportsByCategory = BTreeMap::new();
        let mut module_imports: BTreeSet<String> = BTreeSet::new();

        for import in &self.imports {
            if import.name.is_some() {
                imports_by_category
                    .entry(import.category.clone())
                    .or_default()
                    .entry(import.module.clone())
                    .or_default()
                    .insert((import.name.clone(), import.alias.clone()));
            } else {
                module_imports.insert(format!(
                    "import {} as {}",
                    import.module,
                    import.alias.as_ref().unwrap()
                ));
            }
        }

        let mut sorted_module_imports: Vec<_> = module_imports.iter().collect();
        sorted_module_imports.sort();
        for import in sorted_module_imports {
            printer.text(import).endl();
        }

        let mut first_category = true;
        for (category, modules) in imports_by_category {
            if !first_category
                || (!module_imports.is_empty() && category == ImportCategory::ThirdParty)
            {
                printer.endl();
            }
            first_category = false;
            if category == ImportCategory::Local {
                printer.endl();
            }
            for (module, names) in modules {
                printer.text("from ").text(module).text(" import ");
                let mut sorted_names: Vec<_> = names.iter().collect();
                sorted_names.sort();
                for (i, (name, alias)) in sorted_names.iter().enumerate() {
                    if i > 0 {
                        printer.text(", ");
                    }
                    printer.text(name.as_ref().unwrap());
                    if let Some(a) = alias {
                        printer.text(" as ").text(a);
                    }
                }
                printer.endl();
            }
        }
    }
}

/// Whether a definition referenced from an eager module-level statement resolves to something
/// this backend never renders. Alias right-hand sides and class bases evaluate at import time,
/// so an alias whose type resolves to an unrendered kind would raise `NameError` on emission.
/// Struct field annotations are deferred strings and are not affected.
pub(crate) fn def_is_dead(hir: &ResolvedGraph, def_id: DefId) -> bool {
    let def = hir.context.definitions.get(def_id);
    match &def.kind {
        DefKind::Alias(alias_ty) => ty_is_dead(hir, &alias_ty.ty),
        DefKind::Struct(_) | DefKind::Union(_) | DefKind::Enum(_) => false,
        _ => true,
    }
}

fn ty_is_dead(hir: &ResolvedGraph, ty: &Ty) -> bool {
    match &ty.kind {
        TyKind::Adt(def_id) => def_is_dead(hir, *def_id),
        TyKind::Sequence { ty, .. } | TyKind::Array { ty, .. } => ty_is_dead(hir, ty),
        TyKind::Map { key, elem, .. } => ty_is_dead(hir, key) || ty_is_dead(hir, elem),
        _ => false,
    }
}

pub(crate) struct PydanticWriter<'a> {
    hir: &'a ResolvedGraph,
    options: &'a PydanticOptions,
    module_paths: &'a BTreeMap<DefId, String>,
    imports: ImportManager<'a>,
    namespace: String,
}

impl<'a> PydanticWriter<'a> {
    pub(crate) fn new(
        hir: &'a ResolvedGraph,
        module_paths: &'a BTreeMap<DefId, String>,
        current_module_path: &'a str,
        namespace: String,
        options: &'a PydanticOptions,
        idl_stem: &'a str,
    ) -> Self {
        Self {
            hir,
            options,
            module_paths,
            imports: ImportManager::new(options, current_module_path, idl_stem),
            namespace,
        }
    }

    fn add_adt_import(&mut self, def_id: DefId) {
        if let Some(path) = self.module_paths.get(&def_id) {
            self.imports.add_adt_import(path);
        }
    }

    pub(crate) fn uses_cidl_helper(&self) -> bool {
        self.imports.uses_cidl_helper()
    }

    pub(crate) fn render_module(&mut self, def_ids: &[DefId]) -> String {
        let mut printer = PrettyPrinter::new();
        printer.text("from __future__ import annotations").endl();

        let mut body_printer = PrettyPrinter::new();
        let mut sorted_def_ids = def_ids.to_vec();
        let mut exported_names = Vec::new();

        // Sort: Const, Enum < Struct, Union < Alias
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
            if matches!(&def.kind, DefKind::Alias(_) if def_is_dead(self.hir, def_id)) {
                continue;
            }
            match &def.kind {
                DefKind::Struct(_) | DefKind::Enum(_) | DefKind::Alias(_) => {
                    exported_names.push(Self::escape_python_keyword(&def.ident.name));
                }
                DefKind::Union(union_ty) => {
                    exported_names.push(Self::escape_python_keyword(&def.ident.name));
                    for variant in &union_ty.variants {
                        if variant.ident.name != "_implicit_default" {
                            exported_names.push(Self::union_variant_class_name(def, variant));
                        }
                    }
                }
                _ => {}
            }
            self.render_def(&mut body_printer, def);
        }

        self.imports.render(&mut printer);

        let body = body_printer.finish();
        if !self.imports.imports.is_empty() && !body.is_empty() {
            printer.endl();
        }

        printer.text(&body);

        if !exported_names.is_empty() {
            printer.endl();
            printer.text("__all__ = [").endl();
            printer.indent();
            for name in exported_names {
                printer.text("\"").text(&name).text("\",").endl();
            }
            printer.dedent();
            printer.text("]").endl();
        }

        printer.finish()
    }

    fn render_def(&mut self, printer: &mut PrettyPrinter, def: &Def) {
        match &def.kind {
            DefKind::Struct(struct_ty) => self.render_struct(printer, def, struct_ty),
            DefKind::Union(union_ty) => self.render_union(printer, def, union_ty),
            DefKind::Enum(enum_ty) => self.render_enum(printer, def, enum_ty),
            DefKind::Alias(alias_ty) => self.render_alias(printer, def, alias_ty),
            _ => {}
        }
    }

    fn get_doc_from_annotations(annotations: &[Ann]) -> Option<String> {
        let docs: Vec<String> = annotations
            .iter()
            .filter(|ann| {
                Self::annotation_matches(&ann.ident.name, "doc")
                    || Self::annotation_matches(&ann.ident.name, "documentation")
            })
            .filter_map(|ann| {
                ann.args.first().and_then(|arg| {
                    if let Numeric::String(s) = &arg.value {
                        Some(s.clone())
                    } else {
                        None
                    }
                })
            })
            .collect();

        if docs.is_empty() {
            None
        } else {
            Some(docs.join("\n"))
        }
    }

    fn render_docstring(printer: &mut PrettyPrinter, annotations: &[Ann]) {
        if let Some(doc) = Self::get_doc_from_annotations(annotations) {
            let trimmed = doc.trim();
            if !trimmed.is_empty() {
                printer.text("\"\"\"").endl();
                for line in trimmed.lines() {
                    let line = line.trim();
                    if line.is_empty() {
                        printer.endl();
                    } else {
                        printer.text(Self::escape_docstring_line(line)).endl();
                    }
                }
                printer.text("\"\"\"").endl();
            }
        }
    }

    fn render_adt_name(&self, def_id: DefId) -> String {
        let def = self.hir.context.type_of(def_id);
        let name = Self::escape_python_keyword(&def.ident.name);

        // A def outside the collected module tree has no import alias, so only the bare name can be emitted.
        let Some(module_path) = self.module_paths.get(&def_id) else {
            return name;
        };

        if module_path.as_str() == self.imports.own_module_path {
            return name;
        }

        let alias = format!("{}_", module_path.replace('.', "_"));
        format!("{alias}.{name}")
    }

    fn render_struct(&mut self, printer: &mut PrettyPrinter, def: &Def, struct_ty: &StructTy) {
        let base = if let Some(parent_id) = struct_ty.parent {
            let base_id = self.base_def_id(parent_id.def_id);
            self.add_adt_import(base_id);
            self.render_adt_name(base_id)
        } else {
            let model_base_class = self
                .options
                .model_base_class
                .as_deref()
                .unwrap_or("pydantic.BaseModel");
            if let Some((module, name)) = model_base_class.rsplit_once('.') {
                self.imports.add_import(module, name, "_BaseModel_");
            }
            "_BaseModel_".to_string()
        };

        printer.blank();
        let name = Self::escape_python_keyword(&def.ident.name);
        printer
            .text("class ")
            .text(&name)
            .text("(")
            .text(&base)
            .text("):")
            .endl();
        printer.indent();

        Self::render_docstring(printer, &def.annotations);

        if Self::has_aliased_members(&struct_ty.members) {
            self.render_model_config(printer);
        }

        if struct_ty.members.is_empty() {
            printer.text("pass").endl();
        } else {
            for member in &struct_ty.members {
                self.render_field(printer, member);
            }
        }

        printer.dedent();
    }

    /// A base clause evaluates at import time before later alias assignments, so a base named by
    /// an alias is emitted as the definition the alias names, one hop.
    fn base_def_id(&self, def_id: DefId) -> DefId {
        let def = self.hir.context.definitions.get(def_id);
        if let DefKind::Alias(alias_ty) = &def.kind
            && let TyKind::Adt(target) = &alias_ty.ty.kind
        {
            return *target;
        }
        def_id
    }

    // A field named like its own annotation resolves to the field itself when pydantic
    // evaluates the annotation, so the annotation vocabulary of this writer is escaped too.
    pub(crate) fn escape_python_keyword(name: &str) -> String {
        match name {
            "False" | "None" | "True" | "and" | "as" | "assert" | "async" | "await" | "break"
            | "class" | "continue" | "def" | "del" | "elif" | "else" | "except" | "finally"
            | "for" | "from" | "global" | "if" | "import" | "in" | "is" | "lambda" | "nonlocal"
            | "not" | "or" | "pass" | "raise" | "return" | "try" | "while" | "with" | "yield"
            | "match" | "case" | "bool" | "bytes" | "dict" | "float" | "int" | "list" | "str" => {
                format!("{name}_")
            }
            _ => name.to_string(),
        }
    }

    fn has_aliased_members(members: &[Member]) -> bool {
        members
            .iter()
            .any(|member| Self::escape_python_keyword(&member.ident.name) != member.ident.name)
    }

    fn render_model_config(&mut self, printer: &mut PrettyPrinter) {
        self.imports
            .add_import("pydantic", "ConfigDict", "_ConfigDict_");
        printer
            .text("model_config = _ConfigDict_(validate_by_name=True, validate_by_alias=True)")
            .endl();
    }

    fn render_field(&mut self, printer: &mut PrettyPrinter, member: &Member) {
        if let Some(doc) = Self::get_doc_from_annotations(&member.annotations) {
            for line in doc.trim().lines() {
                printer.text("#: ").text(line.trim()).endl();
            }
        }

        let optional = Self::is_optional(&member.annotations);
        let ty_repr = if optional {
            self.imports.add_module_import("typing", "_typing_");
            format!("_typing_.Optional[{}]", self.render_type(&member.ty))
        } else {
            self.render_type(&member.ty)
        };

        let mut field_args = Vec::new();
        if optional {
            field_args.push("default=None".to_string());
        }

        if let Some(default) = Self::get_default_value(&member.annotations) {
            field_args.push(format!("default={}", self.numeric_literal(&default)));
        } else if !optional
            && !self.options.no_implicit_defaults
            && let Some(default_val) = self.get_implicit_default(&member.ty)
        {
            field_args.push(format!("default={default_val}"));
        }

        self.add_validation_rules(&member.annotations, &member.ty, &mut field_args);

        field_args.push(format!(
            "json_schema_extra={{\"namespace\": \"{}\"}}",
            self.namespace
        ));

        let name = Self::escape_python_keyword(&member.ident.name);
        if name != member.ident.name {
            field_args.push(format!("alias=\"{}\"", member.ident.name));
        }

        self.imports.add_import("pydantic", "Field", "_Field_");
        printer
            .text(&name)
            .text(": ")
            .text(&ty_repr)
            .text(" = _Field_(")
            .text(field_args.join(", "))
            .text(")")
            .endl();
    }

    fn render_type(&mut self, ty: &Ty) -> String {
        match &ty.kind {
            TyKind::Primitive(primitive) => Self::primitive_to_python(*primitive).to_string(),
            TyKind::String { .. } => "str".to_string(),
            TyKind::Sequence { ty, .. } => {
                self.imports.add_module_import("typing", "_typing_");
                format!("_typing_.List[{}]", self.render_type(ty))
            }
            TyKind::Array { ty, .. } => {
                if self.is_primitive(ty, PrimitiveTy::UInt8) {
                    "bytes".to_string()
                } else if self.is_primitive(ty, PrimitiveTy::Char) {
                    "str".to_string()
                } else {
                    self.imports.add_module_import("typing", "_typing_");
                    format!("_typing_.List[{}]", self.render_type(ty))
                }
            }
            TyKind::Map { key, elem, .. } => {
                self.imports.add_module_import("typing", "_typing_");
                let key_ty = self.render_type(key);
                let elem_ty = self.render_type(elem);
                format!("_typing_.Dict[{key_ty}, {elem_ty}]")
            }
            TyKind::Any => {
                self.imports.add_module_import("typing", "_typing_");
                "_typing_.Any".to_string()
            }
            TyKind::Null => "None".to_string(),
            TyKind::Fixed => "bytes".to_string(),
            TyKind::Adt(def_id) => {
                self.add_adt_import(*def_id);
                self.render_adt_name(*def_id)
            }
        }
    }

    fn enum_value_mode(&self, enum_ty: &EnumTy) -> EnumValueMode {
        if enum_ty.fields.iter().any(|&field_id| {
            let variant_def = self.hir.context.type_of(field_id);
            variant_def.flags.contains(DefFlags::IS_ENUMERATED)
        }) {
            return EnumValueMode::Declared;
        }
        match self.options.enum_defaults.as_deref() {
            Some("literal-int") => EnumValueMode::LiteralInt,
            Some("auto-str") => EnumValueMode::AutoStr,
            Some("auto-int") => EnumValueMode::AutoInt,
            _ => EnumValueMode::LiteralStr,
        }
    }

    fn render_enum(&mut self, printer: &mut PrettyPrinter, def: &Def, enum_ty: &EnumTy) {
        let mode = self.enum_value_mode(enum_ty);

        let mut bases: Vec<String> = Vec::new();

        if let Some(enum_base_class) = &self.options.enum_base_class {
            if let Some((module, name)) = enum_base_class.rsplit_once('.') {
                self.imports.add_import(module, name, "_BaseEnum_");
            }
        } else {
            self.imports.add_module_import("enum", "_enum_");
        }

        if !self.options.enum_no_primitive_base {
            bases.push(match mode {
                EnumValueMode::Declared => Self::primitive_to_python(enum_ty.ty).to_string(),
                EnumValueMode::LiteralInt | EnumValueMode::AutoInt => "int".to_string(),
                EnumValueMode::LiteralStr | EnumValueMode::AutoStr => "str".to_string(),
            });
        }

        if self.options.enum_base_class.is_some() {
            bases.push("_BaseEnum_".to_string());
        } else {
            bases.push("_enum_.Enum".to_string());
        }

        printer.blank();

        let schema_mode = self
            .options
            .enum_schema_override
            .as_deref()
            .unwrap_or_else(|| {
                if self.options.enum_base_class.is_some() {
                    "none"
                } else if mode == EnumValueMode::Declared {
                    "value"
                } else {
                    "name"
                }
            });

        if schema_mode != "none" {
            self.imports.add_helper_import();
            let serialize_by_name = schema_mode == "name";
            printer
                .text("@_cidl_enum_schema_(serialize_by_name=")
                .text(if serialize_by_name { "True" } else { "False" })
                .text(")")
                .endl();
        }

        printer
            .text("class ")
            .text(Self::escape_python_keyword(&def.ident.name))
            .text("(")
            .text(bases.join(", "))
            .text("):")
            .endl();
        printer.indent();

        Self::render_docstring(printer, &def.annotations);

        for &field_id in &enum_ty.fields {
            let variant_def = self.hir.context.type_of(field_id);
            if let DefKind::Const(const_ty) = &variant_def.kind {
                self.render_enum_variant(printer, variant_def, const_ty, mode);
            }
        }

        printer.dedent();
    }

    fn render_enum_variant(
        &mut self,
        printer: &mut PrettyPrinter,
        def: &Def,
        const_ty: &ConstTy,
        mode: EnumValueMode,
    ) {
        if let Some(doc) = Self::get_doc_from_annotations(&def.annotations) {
            for line in doc.trim().lines() {
                printer.text("#: ").text(line.trim()).endl();
            }
        }

        // An enum with declared values gets an int primitive base, so every
        // member of such an enum emits its evaluated value; a string member
        // would not construct under the int base.
        let value = match mode {
            EnumValueMode::Declared | EnumValueMode::LiteralInt => {
                self.numeric_literal(&const_ty.value)
            }
            EnumValueMode::AutoStr | EnumValueMode::AutoInt => {
                self.imports.add_module_import("enum", "_enum_");
                "_enum_.auto()".to_string()
            }
            EnumValueMode::LiteralStr => format!("\"{}\"", def.ident.name),
        };
        printer
            .text(Self::escape_python_keyword(&def.ident.name))
            .text(" = ")
            .text(&value)
            .endl();
    }

    fn render_alias(&mut self, printer: &mut PrettyPrinter, def: &Def, alias_ty: &AliasTy) {
        printer.blank();
        if let Some(doc) = Self::get_doc_from_annotations(&def.annotations) {
            for line in doc.trim().lines() {
                printer.text("#: ").text(line.trim()).endl();
            }
        }
        let ty = self.render_type(&alias_ty.ty);

        let mut rules = self.get_validation_rules(&def.annotations, &alias_ty.ty);
        rules.extend(self.get_type_validation_rules(&alias_ty.ty));

        let name = Self::escape_python_keyword(&def.ident.name);
        if rules.is_empty() {
            printer.text(&name).text(" = ").text(&ty).endl();
            return;
        }

        self.imports.add_module_import("typing", "_typing_");
        printer
            .text(&name)
            .text(" = _typing_.Annotated[")
            .text(&ty)
            .text(", ");

        if ty == "str" {
            self.imports
                .add_import("pydantic", "StringConstraints", "_StringConstraints_");
            printer
                .text("_StringConstraints_(")
                .text(rules.join(", "))
                .text(")");
        } else {
            self.imports.add_import("pydantic", "Field", "_Field_");
            printer.text("_Field_(").text(rules.join(", ")).text(")");
        }

        printer.text("]").endl();
    }

    fn render_union(&mut self, printer: &mut PrettyPrinter, def: &Def, union_ty: &UnionTy) {
        let value_to_def_id = if let TyKind::Adt(switch_ty_def_id) = &union_ty.disc.ty.kind {
            let discriminator_enum = self.hir.context.type_of(*switch_ty_def_id);
            let mut map = BTreeMap::new();
            if let DefKind::Enum(enum_ty) = &discriminator_enum.kind {
                for &field_id in &enum_ty.fields {
                    let field_def = self.hir.context.type_of(field_id);
                    if let DefKind::Const(const_ty) = &field_def.kind {
                        let val = self.hir.context.integer_value(&const_ty.value).to_string();
                        map.insert(val, Some(field_id));
                    }
                }
            }
            Some(map)
        } else {
            None
        };

        let variants: Vec<_> = union_ty
            .variants
            .iter()
            .filter(|v| v.ident.name != "_implicit_default")
            .collect();

        let emit_discriminator = variants.iter().all(|variant| {
            !variant.labels.is_empty()
                && !self
                    .collect_discriminator_values(variant, value_to_def_id.as_ref())
                    .is_empty()
        });

        for variant in &variants {
            self.render_union_variant_class(
                printer,
                def,
                union_ty,
                variant,
                value_to_def_id.as_ref(),
            );
        }

        printer.blank();
        if let Some(doc) = Self::get_doc_from_annotations(&def.annotations) {
            for line in doc.trim().lines() {
                printer.text("#: ").text(line.trim()).endl();
            }
        }
        self.imports.add_module_import("typing", "_typing_");
        self.imports.add_import("pydantic", "Field", "_Field_");
        printer
            .text(Self::escape_python_keyword(&def.ident.name))
            .text(" = ");
        if emit_discriminator {
            printer.text("_typing_.Annotated[_typing_.Union[");
        } else {
            printer.text("_typing_.Union[");
        }
        for (i, variant) in variants.iter().enumerate() {
            if i > 0 {
                printer.text(", ");
            }
            printer.text(Self::union_variant_class_name(def, variant));
        }
        printer.text("]");
        if emit_discriminator {
            printer.text(", _Field_(discriminator=\"discriminator\")]");
        }
        printer.endl();
    }

    fn collect_discriminator_values(
        &mut self,
        variant: &Variant,
        value_to_def_id: Option<&BTreeMap<String, Option<DefId>>>,
    ) -> Vec<String> {
        let mut discriminator_values = Vec::new();
        for label in &variant.labels {
            if let Some(map) = value_to_def_id {
                let lookup_key = self.hir.context.integer_value(&label.value).to_string();
                if let Some(Some(discriminator_variant)) = map.get(&lookup_key) {
                    let discriminator_variant_def =
                        self.hir.context.type_of(*discriminator_variant);
                    discriminator_values
                        .push(format!("\"{}\"", discriminator_variant_def.ident.name));
                }
            } else {
                discriminator_values.push(self.get_numeric_value(&label.value));
            }
        }
        discriminator_values
    }

    fn render_union_variant_class(
        &mut self,
        printer: &mut PrettyPrinter,
        union_def: &Def,
        _union_ty: &UnionTy,
        variant: &Variant,
        value_to_def_id: Option<&BTreeMap<String, Option<DefId>>>,
    ) {
        let class_name = Self::union_variant_class_name(union_def, variant);
        let model_base_class = self
            .options
            .model_base_class
            .as_deref()
            .unwrap_or("pydantic.BaseModel");
        if let Some((module, name)) = model_base_class.rsplit_once('.') {
            self.imports.add_import(module, name, "_BaseModel_");
        }
        printer.blank();
        printer
            .text("class ")
            .text(&class_name)
            .text("(_BaseModel_):")
            .endl();
        printer.indent();
        if Self::escape_python_keyword(&variant.ident.name) != variant.ident.name {
            self.render_model_config(printer);
        }
        self.imports.add_module_import("typing", "_typing_");
        self.imports.add_import("pydantic", "Field", "_Field_");

        if !variant.labels.is_empty() {
            let discriminator_values = self.collect_discriminator_values(variant, value_to_def_id);

            if let Some(default_val) = discriminator_values.first() {
                let literal_type = if discriminator_values.len() == 1 {
                    format!("_typing_.Literal[{}]", discriminator_values[0])
                } else {
                    format!("_typing_.Literal[{}]", discriminator_values.join(", "))
                };

                let discriminator_field = format!(
                    "discriminator: {} = _Field_(alias=\"$discriminator\", default={}, \
                     json_schema_extra={{\"namespace\": \"{}\"}})",
                    literal_type, default_val, self.namespace
                );
                printer.text(&discriminator_field).endl();
            }
        }

        let member = Member {
            ident: variant.ident.clone(),
            ty: variant.ty.clone(),
            annotations: variant.annotations.clone(),
        };
        self.render_field(printer, &member);
        printer.dedent();
    }

    fn union_variant_class_name(union_def: &Def, variant: &Variant) -> String {
        format!(
            "{}{}",
            case::pascal(&union_def.ident.name),
            case::pascal(&variant.ident.name)
        )
    }

    fn get_implicit_default(&mut self, ty: &Ty) -> Option<String> {
        match &ty.kind {
            TyKind::Primitive(prim) => match prim {
                PrimitiveTy::Bool => Some("False".to_string()),
                PrimitiveTy::Char | PrimitiveTy::WChar => Some("''".to_string()),
                PrimitiveTy::Int8
                | PrimitiveTy::UInt8
                | PrimitiveTy::Int16
                | PrimitiveTy::UInt16
                | PrimitiveTy::Int32
                | PrimitiveTy::UInt32
                | PrimitiveTy::Int64
                | PrimitiveTy::UInt64 => Some("0".to_string()),
                PrimitiveTy::Float32 | PrimitiveTy::Float64 | PrimitiveTy::Float128 => {
                    Some("0.0".to_string())
                }
                PrimitiveTy::Void => None,
            },
            TyKind::String { .. } => Some("\"\"".to_string()),
            TyKind::Adt(def_id) => {
                let def = self.hir.context.type_of(*def_id);
                if let DefKind::Enum(enum_ty) = &def.kind {
                    if let Some(&first) = enum_ty.fields.first() {
                        Some(self.enum_constant(first))
                    } else {
                        None
                    }
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn get_numeric_value(&mut self, numeric: &Numeric) -> String {
        match numeric {
            Numeric::Const(def_id) => {
                let def = self.hir.context.type_of(*def_id);
                if let DefKind::Const(const_ty) = &def.kind {
                    self.get_numeric_value(&const_ty.value)
                } else {
                    panic!("Expected a const")
                }
            }
            _ => self.numeric_literal(numeric),
        }
    }

    fn is_optional(annotations: &[Ann]) -> bool {
        annotations
            .iter()
            .any(|ann| Self::annotation_matches(&ann.ident.name, "optional"))
    }

    fn get_default_value(annotations: &[Ann]) -> Option<Numeric> {
        annotations.iter().find_map(|ann| {
            if Self::annotation_matches(&ann.ident.name, "default") {
                ann.args.first().map(|arg| arg.value.clone())
            } else {
                None
            }
        })
    }

    fn builtin_bound_rule(
        &mut self,
        annotations: &[Ann],
        name: &str,
        keyword: &str,
    ) -> Option<String> {
        let annotation = builtin_annotation(&self.hir.context, annotations, name)?;
        let arg = annotation.args.first()?;
        let value = self.numeric_literal(&arg.value);
        Some(format!("{keyword}={value}"))
    }

    fn get_validation_rules(&mut self, annotations: &[Ann], ty: &Ty) -> Vec<String> {
        let mut rules: Vec<String> = Vec::new();
        let type_axis = self.type_axis(ty);

        for (name, keyword, axis) in [
            ("min", "ge", Axis::Numeric),
            ("max", "le", Axis::Numeric),
            ("exclusive_minimum", "gt", Axis::Numeric),
            ("exclusive_maximum", "lt", Axis::Numeric),
            ("min_length", "min_length", Axis::Length),
            ("max_length", "max_length", Axis::Length),
            ("min_items", "min_length", Axis::Length),
        ] {
            if type_axis != Some(axis)
                || rules
                    .iter()
                    .any(|rule| rule.split('=').next() == Some(keyword))
            {
                continue;
            }
            rules.extend(self.builtin_bound_rule(annotations, name, keyword));
        }

        if let Some(annotation) = annotations
            .iter()
            .find(|ann| ann.ident.name == "ext::pattern_python")
        {
            self.imports.add_module_import("re", "_re_");
            if let Some(arg) = annotation.args.first() {
                let pattern = self.numeric_literal_ext(&arg.value, true);
                rules.push(format!("pattern=_re_.compile(r'{pattern}')"));
            }
        }

        rules
    }

    #[allow(clippy::unused_self)]
    fn get_type_validation_rules(&self, ty: &Ty) -> Vec<String> {
        match &ty.kind {
            TyKind::Array { len, .. } => {
                vec![format!("min_length={len}"), format!("max_length={len}")]
            }
            _ => Vec::new(),
        }
    }

    fn add_validation_rules(&mut self, annotations: &[Ann], ty: &Ty, field_args: &mut Vec<String>) {
        field_args.extend(self.get_validation_rules(annotations, ty));
        field_args.extend(self.get_type_validation_rules(ty));
    }

    fn annotation_matches(name: &str, expected: &str) -> bool {
        name.rsplit("::").next() == Some(expected)
    }

    #[allow(clippy::unused_self)]
    fn is_primitive(&self, ty: &Ty, expected: PrimitiveTy) -> bool {
        if let TyKind::Primitive(prim) = &ty.kind {
            *prim == expected
        } else {
            false
        }
    }

    fn type_axis(&self, ty: &Ty) -> Option<Axis> {
        match &ty.kind {
            TyKind::Primitive(
                PrimitiveTy::Int8
                | PrimitiveTy::UInt8
                | PrimitiveTy::Int16
                | PrimitiveTy::UInt16
                | PrimitiveTy::Int32
                | PrimitiveTy::UInt32
                | PrimitiveTy::Int64
                | PrimitiveTy::UInt64
                | PrimitiveTy::Float32
                | PrimitiveTy::Float64
                | PrimitiveTy::Float128,
            ) => Some(Axis::Numeric),
            TyKind::String { .. } | TyKind::Sequence { .. } | TyKind::Map { .. } => {
                Some(Axis::Length)
            }
            TyKind::Adt(def_id) => match &self.hir.context.type_of(*def_id).kind {
                DefKind::Alias(alias) => self.type_axis(&alias.ty),
                _ => None,
            },
            // A fixed array length is type-determined; annotation bounds are out-of-axis.
            _ => None,
        }
    }

    fn primitive_to_python(primitive: PrimitiveTy) -> &'static str {
        match primitive {
            PrimitiveTy::Void => "None",
            PrimitiveTy::Bool => "bool",
            PrimitiveTy::Char | PrimitiveTy::WChar => "str",
            PrimitiveTy::Int8
            | PrimitiveTy::UInt8
            | PrimitiveTy::Int16
            | PrimitiveTy::UInt16
            | PrimitiveTy::Int32
            | PrimitiveTy::UInt32
            | PrimitiveTy::Int64
            | PrimitiveTy::UInt64 => "int",
            PrimitiveTy::Float32 | PrimitiveTy::Float64 | PrimitiveTy::Float128 => "float",
        }
    }

    fn numeric_literal_ext(&mut self, numeric: &Numeric, raw_string: bool) -> String {
        match numeric {
            Numeric::Null => "None".to_string(),
            Numeric::Bool(value) => (if *value { "True" } else { "False" }).to_string(),
            Numeric::Char(value) | Numeric::WChar(value) => format!("'{value}'"),
            Numeric::Int8(value) => value.to_string(),
            Numeric::UInt8(value) => value.to_string(),
            Numeric::Int16(value) => value.to_string(),
            Numeric::UInt16(value) => value.to_string(),
            Numeric::Int32(value) => value.to_string(),
            Numeric::UInt32(value) => value.to_string(),
            Numeric::Int64(value) => value.to_string(),
            Numeric::UInt64(value) => value.to_string(),
            Numeric::Float(value) => value.to_string(),
            Numeric::Double(value) => value.to_string(),
            Numeric::String(value) | Numeric::WString(value) => {
                if raw_string {
                    value.clone()
                } else {
                    Self::format_string_literal(value)
                }
            }
            Numeric::Const(def_id) => self.enum_constant(*def_id),
            Numeric::Array { values, .. } | Numeric::Sequence { values, .. } => {
                let items = values
                    .iter()
                    .map(|value| self.numeric_literal(value))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{items}]")
            }
            Numeric::Map { entries, .. } => {
                let items = entries
                    .iter()
                    .map(|(key, value)| {
                        format!(
                            "{}: {}",
                            self.numeric_literal(key),
                            self.numeric_literal(value)
                        )
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{{{items}}}")
            }
            Numeric::Struct { ty, fields } => {
                self.add_adt_import(*ty);
                let name = self.render_adt_name(*ty);
                let members = match &self.hir.context.type_of(*ty).kind {
                    DefKind::Struct(struct_ty) => struct_ty
                        .members
                        .iter()
                        .map(|member| Self::escape_python_keyword(&member.ident.name))
                        .collect::<Vec<_>>(),
                    _ => Vec::new(),
                };
                let items = members
                    .iter()
                    .zip(fields.iter())
                    .map(|(member, value)| format!("{member}={}", self.numeric_literal(value)))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{name}({items})")
            }
            Numeric::Union {
                ty,
                field_index,
                value,
                ..
            } => {
                let variant = match &self.hir.context.type_of(*ty).kind {
                    DefKind::Union(union_ty) => {
                        union_ty.variants.get(*field_index).map(|variant| {
                            (
                                Self::union_variant_class_name(
                                    self.hir.context.type_of(*ty),
                                    variant,
                                ),
                                Self::escape_python_keyword(&variant.ident.name),
                            )
                        })
                    }
                    _ => None,
                };
                match variant {
                    Some((class, field)) => {
                        format!("{class}({field}={})", self.numeric_literal(value))
                    }
                    None => "None".to_string(),
                }
            }
        }
    }

    fn numeric_literal(&mut self, numeric: &Numeric) -> String {
        self.numeric_literal_ext(numeric, false)
    }

    fn enum_constant(&mut self, def_id: DefId) -> String {
        let def = self.hir.context.type_of(def_id);
        let parent_id = def.parent.unwrap();
        self.add_adt_import(parent_id);
        format!(
            "{}.{}",
            self.render_adt_name(parent_id),
            Self::escape_python_keyword(&def.ident.name)
        )
    }

    fn format_string_literal(s: &str) -> String {
        let mut out = String::with_capacity(s.len() + 2);
        out.push('"');
        Self::escape_python_str(s, &mut out, true);
        out.push('"');
        out
    }

    fn escape_docstring_line(s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        Self::escape_python_str(s, &mut out, false);
        out.replace("\"\"\"", "\\\"\"\"")
    }

    fn escape_python_str(s: &str, out: &mut String, escape_double_quote: bool) {
        for c in s.chars() {
            match c {
                '"' if escape_double_quote => out.push_str("\\\""),
                '\\' => out.push_str("\\\\"),
                '\n' => out.push_str("\\n"),
                '\r' => out.push_str("\\r"),
                '\t' => out.push_str("\\t"),
                '\x08' => out.push_str("\\b"),
                '\x0c' => out.push_str("\\f"),
                c if c.is_control() => {
                    use std::fmt::Write;
                    write!(out, "\\u{:04x}", c as u32).unwrap();
                }
                c => out.push(c),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_escape_python_keyword() {
        assert_eq!(PydanticWriter::escape_python_keyword("class"), "class_");
        assert_eq!(PydanticWriter::escape_python_keyword("def"), "def_");
        assert_eq!(PydanticWriter::escape_python_keyword("None"), "None_");
        assert_eq!(PydanticWriter::escape_python_keyword("if"), "if_");
        assert_eq!(PydanticWriter::escape_python_keyword("while"), "while_");
        assert_eq!(PydanticWriter::escape_python_keyword("async"), "async_");
        assert_eq!(PydanticWriter::escape_python_keyword("match"), "match_");
        assert_eq!(PydanticWriter::escape_python_keyword("case"), "case_");
        assert_eq!(PydanticWriter::escape_python_keyword("int"), "int_");
        assert_eq!(PydanticWriter::escape_python_keyword("str"), "str_");
        assert_eq!(PydanticWriter::escape_python_keyword("bytes"), "bytes_");
        assert_eq!(PydanticWriter::escape_python_keyword("normal"), "normal");
    }

    #[test]
    fn test_primitive_to_python() {
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Bool),
            "bool"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Char),
            "str"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::WChar),
            "str"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Int8),
            "int"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::UInt8),
            "int"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Int16),
            "int"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::UInt16),
            "int"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Int32),
            "int"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::UInt32),
            "int"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Int64),
            "int"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::UInt64),
            "int"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Float32),
            "float"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Float64),
            "float"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Float128),
            "float"
        );
        assert_eq!(
            PydanticWriter::primitive_to_python(PrimitiveTy::Void),
            "None"
        );
    }

    #[test]
    fn test_format_string_literal() {
        assert_eq!(PydanticWriter::format_string_literal("abc"), "\"abc\"");
        assert_eq!(PydanticWriter::format_string_literal("a\"b"), "\"a\\\"b\"");
        assert_eq!(PydanticWriter::format_string_literal("a\\b"), "\"a\\\\b\"");
        assert_eq!(PydanticWriter::format_string_literal("a\nb"), "\"a\\nb\"");
        assert_eq!(PydanticWriter::format_string_literal("a\rb"), "\"a\\rb\"");
        assert_eq!(PydanticWriter::format_string_literal("a\tb"), "\"a\\tb\"");
        assert_eq!(PydanticWriter::format_string_literal("a\x08b"), "\"a\\bb\"");
        assert_eq!(PydanticWriter::format_string_literal("a\x0cb"), "\"a\\fb\"");
        assert_eq!(
            PydanticWriter::format_string_literal("a\u{001b}b"),
            "\"a\\u001bb\""
        );
        assert_eq!(PydanticWriter::format_string_literal("a😀b"), "\"a😀b\"");
    }

    #[test]
    fn test_escape_docstring_line() {
        assert_eq!(PydanticWriter::escape_docstring_line("abc"), "abc");
        assert_eq!(PydanticWriter::escape_docstring_line("a\"b"), "a\"b");
        assert_eq!(PydanticWriter::escape_docstring_line("a\\b"), "a\\\\b");
        assert_eq!(PydanticWriter::escape_docstring_line("a\nb"), "a\\nb");
        assert_eq!(PydanticWriter::escape_docstring_line("a\rb"), "a\\rb");
        assert_eq!(PydanticWriter::escape_docstring_line("a\tb"), "a\\tb");
        assert_eq!(PydanticWriter::escape_docstring_line("a\x08b"), "a\\bb");
        assert_eq!(PydanticWriter::escape_docstring_line("a\x0cb"), "a\\fb");
        assert_eq!(
            PydanticWriter::escape_docstring_line("a\u{001b}b"),
            "a\\u001bb"
        );
        assert_eq!(PydanticWriter::escape_docstring_line("a😀b"), "a😀b");
        assert_eq!(
            PydanticWriter::escape_docstring_line("triple \"\"\" quote"),
            "triple \\\"\"\" quote"
        );
    }
}

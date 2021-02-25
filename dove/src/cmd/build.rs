use std::convert::TryFrom;
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;

use anyhow::Error;
use libra::{
    move_lang::{
        compiled_unit,
        errors::{FilesSourceText, output_errors},
    },
    prelude::CompiledUnit,
};
use libra::move_core_types::language_storage::ModuleId;
use libra::prelude::*;
use structopt::StructOpt;
use termcolor::{ColorChoice, StandardStream};

use lang::builder::{Artifacts, MoveBuilder};
use lang::compiler::file::load_move_files;

use crate::cmd::{Cmd, load_dependencies};
use crate::context::Context;

/// Build dependencies.
#[derive(StructOpt, Debug)]
pub struct Build {
    #[structopt(help = "Store modules in json format.", short = "j", long = "json")]
    json: bool,
}

#[derive(Debug, serde::Serialize)]
struct Modules {
    modules: Vec<Module>,
}

#[derive(Debug, serde::Serialize)]
struct Module {
    address: AccountAddress,
    access_vector: String,
    bytecode: String,
}

impl TryFrom<Vec<CompiledUnit>> for Modules {
    type Error = Error;

    fn try_from(modules: Vec<CompiledUnit>) -> Result<Self, Self::Error> {
        let modules = modules.into_iter()
            .filter_map(|module| {
                match &module {
                    CompiledUnit::Module { ident, .. } => {
                        let ident = &ident.0.value;
                        let address = AccountAddress::new(ident.address.to_u8());

                        Some(Identifier::new(ident.name.0.value.as_str())
                            .map(|ident| AccessPath::from(&ModuleId::new(address, ident)))
                            .and_then(|path| {
                                Ok(Module {
                                    address: path.address,
                                    access_vector: hex::encode(path.path),
                                    bytecode: hex::encode(module.serialize()),
                                })
                            }))
                    }
                    CompiledUnit::Script { .. } => { None }
                }
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Modules {
            modules
        })
    }
}

impl Build {
    /// Verify and store compilation results.
    pub fn verify_and_store(
        &self,
        ctx: &Context,
        files: FilesSourceText,
        compiled_units: Vec<CompiledUnit>,
    ) -> Result<(), Error> {
        let (compiled_units, ice_errors) = compiled_unit::verify_units(compiled_units);
        let (modules, scripts): (Vec<_>, Vec<_>) = compiled_units
            .into_iter()
            .partition(|u| matches!(u, CompiledUnit::Module { .. }));

        if !modules.is_empty() {
            let modules_dir = ctx.path_for(&ctx.manifest.layout.module_output);
            if modules_dir.exists() {
                fs::remove_dir_all(&modules_dir)?;
            }
            fs::create_dir_all(&modules_dir)?;

            if self.json {
                let json_path = if let Some(name) = &ctx.manifest.package.name {
                    let mut file = modules_dir.join(name);
                    file.set_extension("json");
                    file
                } else {
                    modules_dir.join("modules.json")
                };

                self.store_json(modules, &json_path)?;
            } else {
                self.store_units(modules, &modules_dir)?;
            }
        }

        if !scripts.is_empty() {
            let scripts_dir = ctx.path_for(&ctx.manifest.layout.script_output);
            if scripts_dir.exists() {
                fs::remove_dir_all(&scripts_dir)?;
            }
            fs::create_dir_all(&scripts_dir)?;

            self.store_units(scripts, &scripts_dir)?;
        }

        if !ice_errors.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Auto);
            output_errors(&mut writer, files, ice_errors);
            Err(anyhow!("could not verify:{}", ctx.project_name()))
        } else {
            Ok(())
        }
    }

    fn store_units(&self, units: Vec<CompiledUnit>, base_dir: &Path) -> Result<(), Error> {
        for (idx, unit) in units.into_iter().enumerate() {
            let mut path = base_dir.join(format!("{}_{}", idx, unit.name()));
            path.set_extension("mv");
            File::create(&path)?.write_all(&unit.serialize())?
        }
        Ok(())
    }

    fn store_json(&self, units: Vec<CompiledUnit>, path: &Path) -> Result<(), Error> {
        if path.exists() {
            fs::remove_file(path)?;
        }

        let modules = Modules::try_from(units)?;
        fs::write(path, serde_json::to_string_pretty(&modules)?)?;
        Ok(())
    }
}

impl Cmd for Build {
    fn apply(self, ctx: Context) -> Result<(), Error> {
        let dirs = ctx.paths_for(&[
            &ctx.manifest.layout.script_dir,
            &ctx.manifest.layout.module_dir,
        ]);

        let mut index = ctx.build_index()?;

        let dep_set = index.make_dependency_set(&dirs)?;
        let dep_list = load_dependencies(dep_set)?;

        let source_list = load_move_files(&dirs)?;

        let sender = ctx.account_address()?;
        let Artifacts { files, prog } =
            MoveBuilder::new(ctx.dialect.as_ref(), Some(sender).as_ref())
                .build(&source_list, &dep_list);

        match prog {
            Err(errors) => {
                let mut writer = StandardStream::stderr(ColorChoice::Auto);
                output_errors(&mut writer, files, errors);
                Err(anyhow!("could not compile:{}", ctx.project_name()))
            }
            Ok(compiled_units) => self.verify_and_store(&ctx, files, compiled_units),
        }
    }
}

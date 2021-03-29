use std::str::FromStr;

use anyhow::Result;
use move_core_types::account_address::AccountAddress;
use move_core_types::gas_schedule::CostTable;
use crate::compiler::source_map::FileOffsetMap;
use std::borrow::Cow;
use std::fmt::Debug;
use crate::compiler::dialects::diem::Diem;
use crate::compiler::dialects::dfinance::DFinance;
use crate::compiler::dialects::pontem::Pontem;

pub mod dfinance;
pub mod diem;
pub mod line_endings;
pub mod pontem;

pub trait Dialect: Debug {
    fn name(&self) -> &str;

    fn parse_address(&self, addr: &str) -> Result<AccountAddress>;

    fn cost_table(&self) -> CostTable;

    fn replace_addresses<'src>(
        &self,
        source_text: &'src str,
        source_map: &mut FileOffsetMap,
    ) -> Cow<'src, str>;
}

#[derive(serde::Deserialize, Debug, Clone, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum DialectName {
    Libra,
    DFinance,
    Pontem,
}

impl DialectName {
    pub fn get_dialect(&self) -> Box<dyn Dialect> {
        match self {
            DialectName::Libra => Box::new(Diem::default()),
            DialectName::DFinance => Box::new(DFinance::default()),
            DialectName::Pontem => Box::new(Pontem::default()),
        }
    }
}

impl FromStr for DialectName {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "libra" => Ok(DialectName::Libra),
            "dfinance" => Ok(DialectName::DFinance),
            "pontem" => Ok(DialectName::Pontem),
            _ => Err(anyhow::format_err!("Invalid dialect {:?}", s)),
        }
    }
}

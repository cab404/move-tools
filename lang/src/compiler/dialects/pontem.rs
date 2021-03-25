use crate::compiler::dialects::Dialect;
use crate::compiler::source_map::FileOffsetMap;
use anyhow::Context;
use anyhow::Result;
use crate::compiler::ss58::{replace_ss58_addresses, ss58_to_libra};
use move_core_types::gas_schedule::CostTable;
use std::borrow::Cow;
use mvm::gas_schedule::cost_table;
use move_core_types::account_address::AccountAddress;

#[derive(Default, Debug)]
pub struct Pontem;

impl Dialect for Pontem {
    fn name(&self) -> &str {
        "pontem"
    }

    fn parse_address(&self, addr: &str) -> Result<AccountAddress> {
        let address_res = if let Ok(diem_addr) = ss58_to_libra(addr) {
            AccountAddress::from_hex_literal(&diem_addr)
        } else if addr.starts_with("0x") {
            AccountAddress::from_hex_literal(addr)
        } else {
            Err(anyhow::anyhow!(
                "Address is not valid libra or pontem address"
            ))
        };
        address_res
            .with_context(|| format!("Address {:?} is not a valid diem/pontem address", addr))
    }

    fn cost_table(&self) -> CostTable {
        cost_table()
    }

    fn replace_addresses<'src>(&self, source_text: &'src str, source_map: &mut FileOffsetMap) -> Cow<'src, str> {
        Cow::Owned(replace_ss58_addresses(&source_text, source_map))
    }
}

use ethers::types::H160;
use lazy_static::lazy_static;
use std::str::FromStr;

pub const POLY_API_KEY: &str = "adc2800b-aebf-eff9-e3f5-b93a040a88c1";
pub const POLY_API_SECRET: &str = "t1rrbJOvLjMFQuFJ61po_Z8Rfk5WhhHsbtzQaZuqHZg=";
pub const POLY_API_PASSPHRASE: &str =
    "c9ca4780654a8851c8355c5b49531a0a2279291dca18283f6db91e4e1bd31184";

pub const PRIVATE_KEY: &str = "0x45db5ff8b6898ac5e4119b88ff189a7f3318a518685a822d038f12a6abe2b8da";
pub const ADDRESS_STR: &str = "0xb48b9192DC52eED724Fa58c66Fa8926d06A3648e";
lazy_static! {
    pub static ref ADDRESS: H160 = H160::from_str(ADDRESS_STR).unwrap();
}

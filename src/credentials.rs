use ethers::types::H160;
use lazy_static::lazy_static;
use std::str::FromStr;

pub const POLY_API_KEY: &str = "db79e748-95ee-36cd-f9ab-00143377acba";
pub const POLY_API_SECRET: &str = "Mcmj_edaTf_mxsPKLOu8yRelIduEbo3BP1W4izj85js=";
pub const POLY_API_PASSPHRASE: &str =
    "c2f920da38ada61ae1ad55f9ee4bfe084fc6f6ade06623488f232a3529940e0f";

pub const PRIVATE_KEY: &str = "0x8dc78334ff702005b631e249d1e02e76e179af634e4c3869add8dc007b4de411";
pub const ADDRESS_STR: &str = "0xB0A60787710f8D6254dC0E304Fc72b6A3907e0C2";
lazy_static! {
    pub static ref ADDRESS: H160 = H160::from_str(ADDRESS_STR).unwrap();
}

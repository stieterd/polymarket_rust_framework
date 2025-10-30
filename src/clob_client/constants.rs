use ethers::types::{Address, H160};

use lazy_static::lazy_static;
use std::str::FromStr;

pub const ZERO_ADDRESS: Address = H160::zero();

// pub const AMOY: u128 = 80002;
pub const POLYGON: u128 = 137;

// pub const NEG_RISK_EXCHANGE: H160 = "0xC5d563A36AE78145C45a50134d48A1215220f80a".parse().unwrap();
lazy_static! {
    pub static ref NEG_RISK_EXCHANGE: H160 =
        H160::from_str("0xC5d563A36AE78145C45a50134d48A1215220f80a").unwrap();
}

pub const HOST: &str = "https://clob.polymarket.com";
pub const L0: u128 = 0;
pub const L1: u128 = 1;
pub const L2: u128 = 2;

pub const FRAC_CENTS: &str = "0.001";
pub const FULL_CENTS: &str = "0.01";

pub const L1_AUTH_UNAVAILABLE: &str = "A private key is needed to interact with this endpoint!";

pub const L2_AUTH_UNAVAILABLE: &str = "API Credentials are needed to interact with this endpoint!";

pub const END_CURSOR: &str = "LTE=";

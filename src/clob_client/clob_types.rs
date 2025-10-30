use std::fmt;

use super::constants::ZERO_ADDRESS;
use ethers::types::Address;
use serde_json::Value;

#[derive(Clone, Debug)]
pub struct ApiCreds {
    pub api_key: String,
    pub api_secret: String,
    pub api_pass: String,
}

#[derive(Clone, Debug)]
pub struct OpenOrderParams {
    pub id: Option<String>,
    pub market: Option<String>,
    pub asset_id: Option<String>,
}

#[derive(Clone, Debug)]
pub enum AssetType {
    Collateral,
    Conditional,
}

impl fmt::Display for AssetType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            AssetType::Collateral => "COLLATERAL",
            AssetType::Conditional => "CONDITIONAL",
        };
        write!(f, "{}", s)
    }
}

#[derive(Clone, Debug)]
pub struct BalanceAllowanceParameters {
    pub asset_type: Option<AssetType>,
    pub token_id: Option<String>,
    pub signature_type: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct RequestArgs<'a> {
    pub method: &'a str,
    pub request_path: &'a str,
    pub body: Option<&'a Value>,
}

#[derive(Clone, Debug)]
pub struct OrderArgs<'a> {
    pub token_id: &'a str,
    pub price: f64,
    pub size: f64,
    pub side: &'a str,
    pub fee_rate_bps: usize, // default 0
    pub nonce: usize,        // default 0
    pub expiration: usize,   // default 0
    pub taker: Address,      // default ZERO_ADDRESS
}

impl<'a> OrderArgs<'a> {
    pub fn new(
        token_id: &'a str,
        price: f64,
        size: f64,
        side: &'a str,
        fee_rate_bps: Option<usize>,
        nonce: Option<usize>,
        expiration: Option<usize>,
        taker: Option<Address>,
    ) -> Self {
        Self {
            token_id,
            price,
            size,
            side,
            fee_rate_bps: fee_rate_bps.unwrap_or(0),
            nonce: nonce.unwrap_or(0),
            expiration: expiration.unwrap_or(0),
            taker: taker.unwrap_or(ZERO_ADDRESS),
        }
    }
}

#[derive(Clone)]
pub struct CreateOrderOptions<'a> {
    pub tick_size: &'a str, // ["0.1", "0.01", "0.001", "0.0001"]
    pub neg_risk: bool,
}

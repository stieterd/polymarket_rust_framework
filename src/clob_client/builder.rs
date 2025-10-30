use ethers::{core::k256::pkcs8::der::asn1::Null, utils::keccak256};
use ethers::{
    types::{Address, H256, U256},
    utils::to_checksum,
};
use lazy_static::lazy_static;
use std::collections::HashMap;
use tiny_keccak::{Hasher, Keccak};

use super::clob_auth::make_domain;
use super::constants::{NEG_RISK_EXCHANGE, POLYGON};
use super::utils::{generate_seed, prepend_zx};
use super::{
    clob_types::{CreateOrderOptions, OrderArgs},
    signer::PolySigner,
};

const BUY: &str = "BUY";
const SELL: &str = "SELL";

const UTILS_BUY: usize = 0;
const UTILS_SELL: usize = 1;

lazy_static! {
    pub static ref ROUND_CONFIG: HashMap<String, RoundConfig> = {
        let mut m = HashMap::new();
        m.insert(
            "0.1".to_string(),
            RoundConfig {
                price: 1,
                size: 2,
                amount: 3,
            },
        );
        m.insert(
            "0.01".to_string(),
            RoundConfig {
                price: 2,
                size: 2,
                amount: 4,
            },
        );
        m.insert(
            "0.001".to_string(),
            RoundConfig {
                price: 3,
                size: 2,
                amount: 5,
            },
        );
        m.insert(
            "0.0001".to_string(),
            RoundConfig {
                price: 4,
                size: 2,
                amount: 6,
            },
        );
        m
    };
}

lazy_static! {
    pub static ref DOMAIN_SEPARATOR_HASH: [u8; 32] = {
        let domain_separator = make_domain(
            Some("Polymarket CTF Exchange"),
            Some("1"),
            Some(U256::from(POLYGON)),
            Some(*NEG_RISK_EXCHANGE),
        );
        domain_separator.struct_hash()
    };
    pub static ref MESSAGE_PREFIX: [u8; 34] = {
        let mut prefix = [0u8; 34];
        prefix[0] = 0x19;
        prefix[1] = 0x01;
        prefix[2..].copy_from_slice(&DOMAIN_SEPARATOR_HASH[..]);
        prefix
    };
}

lazy_static! {
    pub static ref TYPE_HASH: [u8; 32] = compute_type_hash();
}

#[derive(Debug)]
pub struct OrderBuilder {
    pub signer: PolySigner,
    pub sig_type: u64,
    pub funder: Address,
}

pub fn get_order_amounts(
    side: &str,
    size: f64,
    price: f64,
    round_config: &RoundConfig,
) -> Result<(usize, i64, i64), String> {
    let raw_price = round_normal(price, round_config.price);
    if side == BUY {
        let raw_taker_amt = round_down(size, round_config.size);
        let mut raw_maker_amt = raw_taker_amt * raw_price;

        if decimal_places(raw_maker_amt) > round_config.amount {
            raw_maker_amt = round_up(raw_maker_amt, round_config.amount + 4);
            if decimal_places(raw_maker_amt) > round_config.amount {
                raw_maker_amt = round_down(raw_maker_amt, round_config.amount)
            }
        }
        // TODO: this is important data to push to the db
        let maker_amount = to_token_decimals(raw_maker_amt);
        let taker_amount = to_token_decimals(raw_taker_amt);
        Ok((UTILS_BUY, maker_amount, taker_amount))
    } else if side == SELL {
        let raw_maker_amt = round_down(size, round_config.size);
        let mut raw_taker_amt = raw_maker_amt * raw_price;

        if decimal_places(raw_taker_amt) > round_config.amount {
            raw_taker_amt = round_up(raw_taker_amt, round_config.amount + 4);
            if decimal_places(raw_taker_amt) > round_config.amount {
                raw_taker_amt = round_down(raw_taker_amt, round_config.amount);
            }
        }

        let maker_amount = to_token_decimals(raw_maker_amt);
        let taker_amount = to_token_decimals(raw_taker_amt);

        Ok((UTILS_SELL, maker_amount, taker_amount))
    } else {
        Err(format!("order_args.side must be '{}' or '{}'", BUY, SELL))
    }

    // Ok(("".to_string(), 0.2, 0.3))
}

impl OrderBuilder {
    pub fn new(signer: PolySigner, sig_type: Option<u64>, funder: Option<Address>) -> Self {
        Self {
            funder: funder.unwrap_or(signer.address()),
            signer: signer,
            sig_type: sig_type.unwrap_or(0),
        }
    }

    pub fn get_order_amounts(
        &self,
        side: &str,
        size: f64,
        price: f64,
        round_config: &RoundConfig,
    ) -> Result<(usize, i64, i64), String> {
        let raw_price = round_normal(price, round_config.price);
        if side == BUY {
            let raw_taker_amt = round_down(size, round_config.size);
            let mut raw_maker_amt = raw_taker_amt * raw_price;

            if decimal_places(raw_maker_amt) > round_config.amount {
                raw_maker_amt = round_up(raw_maker_amt, round_config.amount + 4);
                if decimal_places(raw_maker_amt) > round_config.amount {
                    raw_maker_amt = round_down(raw_maker_amt, round_config.amount)
                }
            }
            // TODO: this is important data to push to the db
            let maker_amount = to_token_decimals(raw_maker_amt);
            let taker_amount = to_token_decimals(raw_taker_amt);
            Ok((UTILS_BUY, maker_amount, taker_amount))
        } else if side == SELL {
            let raw_maker_amt = round_down(size, round_config.size);
            let mut raw_taker_amt = raw_maker_amt * raw_price;

            if decimal_places(raw_taker_amt) > round_config.amount {
                raw_taker_amt = round_up(raw_taker_amt, round_config.amount + 4);
                if decimal_places(raw_taker_amt) > round_config.amount {
                    raw_taker_amt = round_down(raw_taker_amt, round_config.amount);
                }
            }

            let maker_amount = to_token_decimals(raw_maker_amt);
            let taker_amount = to_token_decimals(raw_taker_amt);

            Ok((UTILS_SELL, maker_amount, taker_amount))
        } else {
            Err(format!("order_args.side must be '{}' or '{}'", BUY, SELL))
        }

        // Ok(("".to_string(), 0.2, 0.3))
    }

    // pub fn create_order2(&self) -> SignedOrder {
    //     return None;
    // }

    pub fn create_order(
        &self,
        order_args: &OrderArgs,
        options: &CreateOrderOptions,
    ) -> SignedOrder {
        let (side, maker_amount, taker_amount) = self
            .get_order_amounts(
                &order_args.side,
                order_args.size,
                order_args.price,
                &ROUND_CONFIG.get(options.tick_size).unwrap(),
            )
            .unwrap();

        let mut data = OrderData {
            maker: Some(self.funder.clone()),
            taker: order_args.taker,
            token_id: Some(&order_args.token_id),
            maker_amount: Some(maker_amount as usize),
            taker_amount: Some(taker_amount as usize),
            side: Some(side as u8),
            fee_rate_bps: Some(order_args.fee_rate_bps as usize),
            nonce: order_args.nonce as usize,
            signer: Some(self.signer.address()),
            expiration: order_args.expiration,
            signature_type: self.sig_type as usize,
        };

        if data.signer.is_none() {
            data.signer = data.maker;
        }

        let order = Order {
            salt: ethers::types::U256::from(generate_seed()),
            maker: data.maker.unwrap(),
            signer: data.signer.unwrap(),
            taker: data.taker,
            token_id: ethers::types::U256::from_dec_str(&data.token_id.unwrap()).unwrap(),
            maker_amount: ethers::types::U256::from(data.maker_amount.unwrap()),
            taker_amount: ethers::types::U256::from(data.taker_amount.unwrap()),
            expiration: ethers::types::U256::from(data.expiration),
            nonce: ethers::types::U256::from(data.nonce),
            fee_rate_bps: ethers::types::U256::from(data.fee_rate_bps.unwrap()),
            side: data.side.unwrap(),
            signature_type: ethers::types::U256::from(data.signature_type),
        };

        let order_struct_hash = compute_order_struct_hash(&order);

        let mut message = Vec::with_capacity(2 + 32 + 32);
        message.extend_from_slice(&MESSAGE_PREFIX[..]);
        message.extend_from_slice(&order_struct_hash);

        let digest = keccak256(&message);

        let digest_h256 = H256::from_slice(&digest);

        let signature = prepend_zx(self.signer.sign(&digest_h256));

        SignedOrder { order, signature }
    }
}

#[derive(Debug, Clone)]
pub struct OrderData<'a> {
    /// Maker of the order, i.e., the source of funds for the order.
    pub maker: Option<Address>,

    /// Address of the order taker. The zero address is used to indicate a public order.
    pub taker: Address,

    /// Token Id of the CTF ERC1155 asset to be bought or sold.
    /// If BUY, this is the tokenId of the asset to be bought, i.e., the makerAssetId.
    /// If SELL, this is the tokenId of the asset to be sold, i.e., the takerAssetId.
    pub token_id: Option<&'a str>,

    /// Maker amount, i.e., the max amount of tokens to be sold.
    pub maker_amount: Option<usize>,

    /// Taker amount, i.e., the minimum amount of tokens to be received.
    pub taker_amount: Option<usize>,

    /// The side of the order, BUY or SELL.
    pub side: Option<u8>,

    /// Fee rate, in basis points, charged to the order maker, charged on proceeds.
    pub fee_rate_bps: Option<usize>,

    /// Nonce used for on-chain cancellations.
    pub nonce: usize,

    /// Signer of the order. Optional, if it is not present the signer is the maker of the order.
    pub signer: Option<Address>,

    /// Timestamp after which the order is expired.
    /// Optional, if it is not present the value is '0' (no expiration).
    pub expiration: usize,

    /// Signature type used by the Order. Default value 'EOA'.
    pub signature_type: usize,
}

#[derive(Debug, Clone)]
pub struct Order {
    pub salt: U256,
    pub maker: Address,
    pub signer: Address,
    pub taker: Address,
    pub token_id: U256,
    pub maker_amount: U256,
    pub taker_amount: U256,
    pub expiration: U256,
    pub nonce: U256,
    pub fee_rate_bps: U256,
    pub side: u8,
    pub signature_type: U256,
}

const ORDER_TYPE: &str = "Order(uint256 salt,address maker,address signer,address taker,uint256 tokenId,uint256 makerAmount,uint256 takerAmount,uint256 expiration,uint256 nonce,uint256 feeRateBps,uint8 side,uint8 signatureType)";

pub fn encode_uint256(value: &U256) -> [u8; 32] {
    let mut buf = [0u8; 32];
    value.to_big_endian(&mut buf);
    buf
}

pub fn encode_uint8(value: u8) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[31] = value;
    buf
}

pub fn encode_address(addr: &Address) -> [u8; 32] {
    let mut buf = [0u8; 32];
    buf[12..].copy_from_slice(addr.as_bytes());
    buf
}

pub fn compute_type_hash() -> [u8; 32] {
    let mut hasher = Keccak::v256();
    hasher.update(ORDER_TYPE.as_bytes());
    let mut output = [0u8; 32];
    hasher.finalize(&mut output);

    output
}

pub fn encode_order(order: &Order) -> Vec<u8> {
    let mut encoded: Vec<u8> = Vec::with_capacity(32 * 12);

    encoded.extend_from_slice(&encode_uint256(&order.salt));
    encoded.extend_from_slice(&encode_address(&order.maker));
    encoded.extend_from_slice(&encode_address(&order.signer));
    encoded.extend_from_slice(&encode_address(&order.taker));
    encoded.extend_from_slice(&encode_uint256(&order.token_id)); // add later
    encoded.extend_from_slice(&encode_uint256(&order.maker_amount)); // add later
    encoded.extend_from_slice(&encode_uint256(&order.taker_amount)); // add later
    encoded.extend_from_slice(&encode_uint256(&order.expiration));
    encoded.extend_from_slice(&encode_uint256(&order.nonce));
    encoded.extend_from_slice(&encode_uint256(&order.fee_rate_bps));
    encoded.extend_from_slice(&encode_uint8(order.side));
    encoded.extend_from_slice(&encode_uint256(&order.signature_type));

    encoded
}

pub fn compute_order_struct_hash(order: &Order) -> [u8; 32] {
    let type_hash = *TYPE_HASH;
    let encoded_values = encode_order(order);

    let mut data = Vec::with_capacity(32 + encoded_values.len());
    data.extend_from_slice(&type_hash);
    data.extend_from_slice(&encoded_values);

    let mut hasher = Keccak::v256();
    hasher.update(&data);
    let mut output = [0u8; 32];
    hasher.finalize(&mut output);

    output
}

impl Order {
    pub fn to_dict(&self) -> HashMap<&str, String> {
        let mut order_map = HashMap::new();
        order_map.insert("salt", self.salt.to_string());
        order_map.insert("maker", to_checksum(&self.maker, None));
        order_map.insert("signer", to_checksum(&self.signer, None));
        order_map.insert("taker", to_checksum(&self.taker, None));
        order_map.insert("tokenId", self.token_id.to_string());
        order_map.insert("makerAmount", self.maker_amount.to_string());
        order_map.insert("takerAmount", self.taker_amount.to_string());
        order_map.insert("expiration", self.expiration.to_string());
        order_map.insert("nonce", self.nonce.to_string());
        order_map.insert("feeRateBps", self.fee_rate_bps.to_string());
        order_map.insert("side", self.side.to_string());
        order_map.insert("signatureType", self.signature_type.to_string());

        order_map
    }
}

#[derive(Debug, Clone)]
pub struct SignedOrder {
    pub order: Order,
    pub signature: String,
}

pub struct RoundConfig {
    price: usize,
    size: usize,
    amount: usize,
}

impl SignedOrder {
    pub fn to_dict(&self) -> HashMap<&str, String> {
        let mut order_map = self.order.to_dict();

        // Add the signature to the dictionary
        order_map.insert("signature", self.signature.to_string());

        // Convert side from integer to string ("BUY" or "SELL")
        let side_str = if order_map["side"] == "0" {
            "BUY"
        } else {
            "SELL"
        };
        order_map.insert("side", side_str.to_string());

        // // Convert other fields to string, as done in the Python version
        // order_map.insert("expiration", self.order.expiration.to_string());
        // order_map.insert("nonce", self.order.nonce.to_string());
        // order_map.insert("feeRateBps", self.order.fee_rate_bps.to_string());
        // order_map.insert("makerAmount", self.order.maker_amount.to_string());
        // order_map.insert("takerAmount", self.order.taker_amount.to_string());
        // order_map.insert("tokenId", self.order.token_id.to_string());

        order_map
    }
}

fn round_down(x: f64, sig_digits: usize) -> f64 {
    (x * 10f64.powi(sig_digits as i32)).floor() / 10f64.powi(sig_digits as i32)
}

fn round_normal(x: f64, sig_digits: usize) -> f64 {
    (x * 10f64.powi(sig_digits as i32)).round() / 10f64.powi(sig_digits as i32)
}

fn round_up(x: f64, sig_digits: usize) -> f64 {
    (x * 10f64.powi(sig_digits as i32)).ceil() / 10f64.powi(sig_digits as i32)
}

fn to_token_decimals(x: f64) -> i64 {
    let mut f = 10f64.powi(6) * x;
    if decimal_places(f) > 0 {
        f = round_normal(f, 0);
    }
    f.round() as i64
}

fn decimal_places(value: f64) -> usize {
    let str_value = value.to_string();
    if let Some(dot_index) = str_value.find('.') {
        str_value.len() - dot_index - 1
    } else {
        0
    }
}

use std::str::FromStr;

use ethers::{
    types::{Address, H256},
    utils::{keccak256, to_checksum},
};
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::{json, Map, Value};
use tiny_keccak::{Hasher, Keccak};

use super::{
    builder::{encode_order, encode_uint256, Order, OrderData, MESSAGE_PREFIX, TYPE_HASH},
    clob_types::ApiCreds,
    constants::ZERO_ADDRESS,
    signer::PolySigner,
    utils::{generate_seed, prepend_zx},
};

const UTILS_BUY: u8 = 0;

#[derive(Debug, Clone)]
pub struct PrebuiltOrder {
    pub headers: HeaderMap,
    pub body: Value,
    pub encoded: Vec<u8>,
    pub message: Vec<u8>,
    pub data: Vec<u8>,
    pub signer: PolySigner,
    pub order: Order,
}

pub fn update_encoded_order(
    encoded: &mut Vec<u8>,
    token_id: &str,
    maker_amount: i64,
    taker_amount: i64,
) {
    // Calculate the starting positions of each field in the encoded vector
    let token_id_start = 32 + 60; // salt (32 bytes) + maker, signer, taker addresses (3 * 20 bytes)
    let maker_amount_start = token_id_start + 32; // token_id (32 bytes)
    let taker_amount_start = maker_amount_start + 32; // maker_amount (32 bytes)

    // Encode the new values
    let token_id_encoded = encode_uint256(&ethers::types::U256::from_dec_str(token_id).unwrap());
    let maker_amount_encoded = encode_uint256(&ethers::types::U256::from(maker_amount));
    let taker_amount_encoded = encode_uint256(&ethers::types::U256::from(taker_amount));

    // Replace the old values with the new encoded values
    encoded[token_id_start..token_id_start + 32].copy_from_slice(&token_id_encoded);
    encoded[maker_amount_start..maker_amount_start + 32].copy_from_slice(&maker_amount_encoded);
    encoded[taker_amount_start..taker_amount_start + 32].copy_from_slice(&taker_amount_encoded);
}

pub fn build_prebuilt_order(
    creds: &ApiCreds,
    signer: &PolySigner,
    funder: Address,
) -> PrebuiltOrder {
    let funder = Some(funder);
    let signer = signer.clone();

    // let (side, maker_amount, taker_amount) = self.get_order_amounts(&order_args.side, order_args.size, order_args.price, &ROUND_CONFIG.get(options.tick_size).unwrap()).unwrap();

    // let order_args = OrderArgs::new(None, None, None, "BUY", None, None, None, None);
    let mut data = OrderData {
        maker: funder,
        taker: ZERO_ADDRESS,
        token_id: Some(
            "51212617306694642074721933439542657092794027549268550682028214048094292871139",
        ), // add later
        maker_amount: Some(0 as usize), // add later
        taker_amount: Some(0 as usize), // add later
        side: Some(UTILS_BUY as u8),
        fee_rate_bps: Some(0 as usize),
        nonce: 0 as usize,
        signer: Some(signer.address()),
        expiration: 0,
        signature_type: 1 as usize,
    };

    let order = Order {
        salt: ethers::types::U256::from(generate_seed()),
        maker: data.maker.unwrap(),
        signer: data.signer.unwrap(),
        taker: data.taker,
        token_id: ethers::types::U256::from_dec_str(&data.token_id.unwrap()).unwrap(), // add later
        maker_amount: ethers::types::U256::from(data.maker_amount.unwrap()),           // add later
        taker_amount: ethers::types::U256::from(data.taker_amount.unwrap()),           // add later
        expiration: ethers::types::U256::from(data.expiration),
        nonce: ethers::types::U256::from(data.nonce),
        fee_rate_bps: ethers::types::U256::from(data.fee_rate_bps.unwrap()),
        side: data.side.unwrap(),
        signature_type: ethers::types::U256::from(data.signature_type),
    };

    let type_hash = *TYPE_HASH;
    let encoded_values = encode_order(&order);
    let mut data = Vec::with_capacity(32 + encoded_values.len());
    data.extend_from_slice(&type_hash);
    // data.extend_from_slice(&encoded_values); -> need to do this after we get them entries correct
    // let mut hasher = Keccak::v256();
    // hasher.update(&data); -> need to do this after we get them entries correct
    // let mut output = [0u8; 32]; -> need to do this after we get them entries correct
    // hasher.finalize(&mut output); -> need to do this after we get them entries correct

    let mut message = Vec::with_capacity(2 + 32 + 32);
    message.extend_from_slice(&MESSAGE_PREFIX[..]);
    // message.extend_from_slice(&order_struct_hash); -> order_struct_hash == output, but its not finalized yet

    // let digest = keccak256(&message); -> need to do this after we get them entries correct

    // let digest_h256 = H256::from_slice(&digest); -> need to do this after we get them entries correct

    // let signature = prepend_zx(signer.sign(digest_h256)); -> need to do this after we get them entries correct

    // Create a mutable JSON map
    let mut order_json_map = Map::new();

    let mut order_map = order.to_dict();

    order_map.insert("signature", "".to_string()); // --> need to do this after we get them entries correct

    let side_str = if order_map["side"] == "0" {
        "BUY"
    } else {
        "SELL"
    };
    order_map.insert("side", side_str.to_string());

    // Define the keys that need to be converted to integers
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_str("POLY_ADDRESS").unwrap(),
        HeaderValue::from_str(&to_checksum(&signer.address(), None)).unwrap(),
    );
    // headers.insert(HeaderName::from_str("POLY_SIGNATURE").unwrap(), HeaderValue::from_str(&hmac_sig).unwrap()); --> need to do this after we get them entries correct
    // headers.insert(HeaderName::from_str("POLY_TIMESTAMP").unwrap(), HeaderValue::from_str(&timestamp).unwrap()); --> need to do this after we get them entries correct
    headers.insert(
        HeaderName::from_str("POLY_API_KEY").unwrap(),
        HeaderValue::from_str(&creds.api_key).unwrap(),
    );
    headers.insert(
        HeaderName::from_str("POLY_PASSPHRASE").unwrap(),
        HeaderValue::from_str(&creds.api_pass).unwrap(),
    );
    headers.insert(
        HeaderName::from_str("User-Agent").unwrap(),
        HeaderValue::from_str("py_clob_client").unwrap(),
    );
    headers.insert(
        HeaderName::from_str("Accept-Encoding").unwrap(),
        HeaderValue::from_str("deflate").unwrap(),
    );
    headers.insert(
        HeaderName::from_str("Accept").unwrap(),
        HeaderValue::from_str("*/*").unwrap(),
    );
    headers.insert(
        HeaderName::from_str("Connection").unwrap(),
        HeaderValue::from_str("keep-alive").unwrap(),
    );
    headers.insert(
        HeaderName::from_str("Content-Type").unwrap(),
        HeaderValue::from_str("application/json").unwrap(),
    );

    let keys_to_convert_to_int = vec!["signatureType", "salt"]; // Replace with the actual keys

    for (k, v) in order_map {
        if keys_to_convert_to_int.contains(&k) {
            // Try to parse the value as an integer
            if let Ok(int_value) = v.parse::<i64>() {
                order_json_map.insert(k.to_string(), Value::Number(int_value.into()));
            } else {
                // If parsing fails, insert as a string
                order_json_map.insert(k.to_string(), Value::String(v));
            }
        } else {
            // Insert as a string for non-integer values
            order_json_map.insert(k.to_string(), Value::String(v));
        }
    }

    // Create the "order" JSON value
    let order_value = Value::Object(order_json_map);

    // Construct the final JSON object
    let body = json!({
        "order": order_value,
        "owner": creds.api_key.clone(),
        "orderType": "GTC"
    });

    let prebuilt_order = PrebuiltOrder {
        headers: headers,
        body: body,
        encoded: encoded_values,
        message: message,
        data: data,
        signer: signer,
        order: order,
    };

    prebuilt_order
}

use super::builder::{get_order_amounts, OrderBuilder, SignedOrder, ROUND_CONFIG};
use super::clob_types::{ApiCreds, CreateOrderOptions, OrderArgs};
use super::constants::{HOST, L2, POLYGON};
use super::endpoints::{CANCEL_ALL, CANCEL_ORDERS};
use super::headers::create_level_2_headers;
use super::hmac::build_hmac_signature;
use super::http_helpers::post;
use super::prebuilt_order::PrebuiltOrder;
use super::signer::PolySigner;
use super::utils::{order_to_json, prepend_zx};
use super::{clob_types::RequestArgs, http_helpers::delete};
use crate::clob_client::builder::encode_order;
use crate::clob_client::clob_types::{BalanceAllowanceParameters, OpenOrderParams};
use crate::clob_client::constants::END_CURSOR;
use crate::clob_client::endpoints::{GET_BALANCE_ALLOWANCE, GET_LAST_TRADES_PRICES, ORDERS};
use crate::clob_client::http_helpers::{
    add_balance_allowance_params_to_url, build_query_params, get,
};
use ethers::abi::token;
use ethers::types::Address;
use ethers::utils::{keccak256, to_checksum};
use num_cpus;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use serde_json::Value;
use std::error::Error;
use std::process;
use std::str::FromStr;
use std::time::{Instant, SystemTime, UNIX_EPOCH};
use tiny_keccak::{Hasher, Keccak};
use tokio::runtime::{Builder, Runtime};
use tokio::task;

fn convert_vec_to_json_value(strings: &[&str]) -> Option<Value> {
    if strings.is_empty() {
        None // If the slice is empty, return None
    } else {
        // Directly convert &[&str] to a JSON array (serde_json::Value)
        serde_json::to_value(strings).ok()
    }
}

#[derive(Debug)]
pub struct ClobClient {
    pub signer: PolySigner,
    // pub key: Option<String>,
    pub creds: ApiCreds,
    pub builder: OrderBuilder,
    // pub signature_type: Option<u128>,
    // pub funder: Option<String>
    pub mode: u128,
    pub checksum_address: String,
    pub high_priority_runtime: Runtime,
}

impl ClobClient {
    // Create a new instance of ClobAuth
    pub fn new(
        key: &str,
        creds: ApiCreds,
        signature_type: Option<u64>,
        funder: Option<Address>,
    ) -> Self {
        let signer = PolySigner::new(key, POLYGON);
        let address_checksum = to_checksum(&signer.address(), None);
        let high_priority_runtime = Builder::new_multi_thread()
            .worker_threads(num_cpus::get()) // Use all available CPU cores
            .enable_all()
            .build()
            .unwrap();
        Self {
            signer: signer.clone(),
            creds: creds,
            mode: L2,
            builder: OrderBuilder::new(signer, signature_type, funder),
            checksum_address: address_checksum,
            high_priority_runtime: high_priority_runtime,
        }
    }

    pub async fn cancel_orders(
        &self,
        order_ids: &[&str],
    ) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let body = convert_vec_to_json_value(order_ids);

        let request_args = RequestArgs {
            method: "DELETE",
            request_path: CANCEL_ORDERS,
            body: body.as_ref(),
        };

        let url = format!("{}{}", HOST, request_args.request_path);
        let headers = create_level_2_headers(&self.signer, &self.creds, &request_args);
        let resp = delete(&url, Some(headers.to_header_map()), body.as_ref()).await;
        match resp {
            Ok(result) => {
                // Treat missing or non-array `not_canceled` as empty slice
                let not_canceled = result
                    .get("not_canceled")
                    .and_then(|v| v.as_array().map(|vec| vec.as_slice()))
                    .unwrap_or(&[]);

                if !not_canceled.is_empty() {
                    println!("there are {} item(s) in `not_canceled`", not_canceled.len());
                    println!("FAIL SAFE HIT");
                    process::exit(1);
                }
                return Ok(result);
            }

            Err(e) => {
                eprintln!("Error in cancelling orders {:?}", e);
                println!("order ids {:?}", order_ids);
                println!("FAIL SAFE HIT");
                process::exit(1);
                return Err(e);
            }
        }
    }

    pub async fn cancel_all(&self) -> Result<Value, Box<dyn Error + Send + Sync>> {
        // return Ok(().into());
        let request_args = RequestArgs {
            method: "DELETE",
            request_path: CANCEL_ALL,
            body: None,
        };
        println!("Cancel all");
        let open_orders = self.get_orders(None, None).await?;
        let mut orders_to_cancel = vec![];
        for order in open_orders.iter() {
            if let Some("No") = order.get("outcome").and_then(Value::as_str) {
                if let Some(id_str) = order.get("id").and_then(Value::as_str) {
                    orders_to_cancel.push(id_str);
                }
            }
        }
        if orders_to_cancel.is_empty() {
            return Ok(().into());
        }
        return self.cancel_orders(orders_to_cancel.as_slice()).await;
        // let url = format!("{}{}", HOST, request_args.request_path);
        // let headers = create_level_2_headers(&self.signer, &self.creds, &request_args);
        // let resp = delete(&url, Some(headers.to_header_map()), None).await;
        // match resp {
        //     Ok(result) => {
        //         return Ok(result);
        //     }
        //     Err(e)=> {
        //         println!("Error in cancelling orders {:?}", e);
        //         println!("FAIL SAFE HIT");
        //         process::exit(1);
        //     }
        // }
        // Ok(().into())
    }

    // pub async fn create_and_post_order()

    pub fn create_order(
        &self,
        order_args: &OrderArgs,
        tick_size: &str,
        neg_risk: bool,
    ) -> SignedOrder {
        let order_options = CreateOrderOptions {
            tick_size: tick_size,
            neg_risk: neg_risk,
        };
        // self.builder.create_order();
        self.builder.create_order(order_args, &order_options)
    }

    pub fn build_prebuilt_order(&self) -> PrebuiltOrder {
        super::prebuilt_order::build_prebuilt_order(&self.creds, &self.signer, self.builder.funder)
    }

    pub async fn post_taker_order(
        &self,
        order: &SignedOrder,
        order_type: &str,
    ) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let body = Some(order_to_json(order, &self.creds.api_key, order_type));
        let request_args = RequestArgs {
            method: "POST",
            request_path: "/order",
            body: body.as_ref(),
        };
        let url = format!("{}{}", HOST, request_args.request_path);

        let timestamp = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            - 115)
            .to_string();

        let hmac_sig = build_hmac_signature(
            &self.creds.api_secret,
            &timestamp,
            &request_args.method,
            &request_args.request_path,
            request_args.body,
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_str("POLY_ADDRESS").unwrap(),
            HeaderValue::from_str(&self.checksum_address).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_SIGNATURE").unwrap(),
            HeaderValue::from_str(&hmac_sig).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_TIMESTAMP").unwrap(),
            HeaderValue::from_str(&timestamp).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_API_KEY").unwrap(),
            HeaderValue::from_str(&self.creds.api_key).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_PASSPHRASE").unwrap(),
            HeaderValue::from_str(&self.creds.api_pass).unwrap(),
        );

        let num_requests = 5;

        let mut handles = Vec::new();

        for _ in 0..num_requests {
            let post_url: String = url.clone();
            let post_headers = Some(headers.clone());
            let post_body = body.clone();

            let handle =
                task::spawn(async move { post(&post_url, post_headers, post_body.as_ref()).await });

            handles.push(handle);
        }

        // Wait for all spawned tasks and collect their results
        let results: Vec<_> = futures::future::join_all(handles).await;
        // println!("results {:?}", results);
        // for (i, res) in results.into_iter().enumerate() {
        // match res {
        // Ok(Ok(response)) =>,// println!("Response {}: {:?}", i, response),
        // Ok(Err(e)) => eprintln!("Request {} failed: {}", i, e),
        // Err(e) => eprintln!("Task {} panicked: {:?}", i, e),
        // }
        // }
        Ok(().into())
    }

    pub async fn post_order(
        &self,
        order: &SignedOrder,
        order_type: &str,
    ) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let body = Some(order_to_json(order, &self.creds.api_key, order_type));

        let request_args = RequestArgs {
            method: "POST",
            request_path: "/order",
            body: body.as_ref(),
        };
        let url = format!("{}{}", HOST, request_args.request_path);

        let timestamp = (SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs()
            - 115)
            .to_string();

        let hmac_sig = build_hmac_signature(
            &self.creds.api_secret,
            &timestamp,
            &request_args.method,
            &request_args.request_path,
            request_args.body,
        );

        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_str("POLY_ADDRESS").unwrap(),
            HeaderValue::from_str(&self.checksum_address).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_SIGNATURE").unwrap(),
            HeaderValue::from_str(&hmac_sig).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_TIMESTAMP").unwrap(),
            HeaderValue::from_str(&timestamp).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_API_KEY").unwrap(),
            HeaderValue::from_str(&self.creds.api_key).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_PASSPHRASE").unwrap(),
            HeaderValue::from_str(&self.creds.api_pass).unwrap(),
        );

        post(&url, Some(headers), body.as_ref()).await
    }

    pub async fn get_balance_allowance(
        &self,
        mut params: BalanceAllowanceParameters,
    ) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let request_args = RequestArgs {
            method: "GET",
            request_path: GET_BALANCE_ALLOWANCE,
            body: None,
        };
        let headers = create_level_2_headers(&self.signer, &self.creds, &request_args);
        let pre_url = format!("{}{}", HOST, request_args.request_path);

        if params.signature_type == None {
            params.signature_type = Some(self.builder.sig_type as i64)
        }

        if let Some(sig_type) = params.signature_type {
            if sig_type == -1 {
                params.signature_type = Some(self.builder.sig_type as i64)
            }
        }

        let url = add_balance_allowance_params_to_url(&pre_url, Some(&params));
        get(&url, Some(headers.to_header_map())).await
    }

    pub async fn get_orders(
        &self,
        params: Option<OpenOrderParams>,
        next_cursor: Option<String>,
    ) -> Result<Vec<Value>, Box<dyn Error + Send + Sync>> {
        let request_args = RequestArgs {
            method: "GET",
            request_path: ORDERS,
            body: None,
        };
        let headers = create_level_2_headers(&self.signer, &self.creds, &request_args);

        let mut results = Vec::new();
        let mut cursor = next_cursor.unwrap_or_else(|| "MA==".to_string());

        while cursor != END_CURSOR {
            let url = add_query_open_orders_params(
                &format!("{}{}", HOST, ORDERS),
                params.as_ref(),
                &cursor,
            );
            let response = get(&url, Some(headers.to_header_map())).await.unwrap();
            cursor = response
                .get("next_cursor")
                .and_then(Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| END_CURSOR.to_string());
            if let Some(data) = response.get("data").and_then(Value::as_array) {
                for item in data {
                    results.push(item.clone());
                }
            }
        }

        Ok(results)
    }
}

pub fn add_query_open_orders_params(
    base_url: &str,
    params: Option<&OpenOrderParams>,
    next_cursor: &str,
) -> String {
    let mut url = base_url.to_string();

    if !next_cursor.is_empty() {
        url.push('?');
        url = build_query_params(&url, "next_cursor", next_cursor);
    }

    if let Some(p) = params {
        // start the query string

        if let Some(ref market) = p.market {
            url = build_query_params(&url, "market", market);
        }
        if let Some(ref asset_id) = p.asset_id {
            url = build_query_params(&url, "asset_id", asset_id);
        }
        if let Some(ref id) = p.id {
            url = build_query_params(&url, "id", id);
        }
    }

    url
}

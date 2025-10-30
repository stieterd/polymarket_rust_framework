use super::clob_types::{ApiCreds, RequestArgs};
use super::hmac::build_hmac_signature;
use super::signer::PolySigner;
use ethers::utils::to_checksum;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

pub struct Headers {
    pub POLY_ADDRESS: String,
    pub POLY_SIGNATURE: String,
    pub POLY_TIMESTAMP: String,
    pub POLY_API_KEY: String,
    pub POLY_PASSPHRASE: String,
}

impl Headers {
    pub fn new(
        poly_address: &str,
        poly_signature: &str,
        poly_timestamp: &str,
        poly_api_key: &str,
        poly_passphrase: &str,
    ) -> Self {
        Self {
            POLY_ADDRESS: poly_address.to_string(),
            POLY_SIGNATURE: poly_signature.to_string(),
            POLY_TIMESTAMP: poly_timestamp.to_string(),
            POLY_API_KEY: poly_api_key.to_string(),
            POLY_PASSPHRASE: poly_passphrase.to_string(),
        }
    }

    pub fn to_header_map(&self) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_str("POLY_ADDRESS").unwrap(),
            HeaderValue::from_str(&self.POLY_ADDRESS).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_SIGNATURE").unwrap(),
            HeaderValue::from_str(&self.POLY_SIGNATURE).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_TIMESTAMP").unwrap(),
            HeaderValue::from_str(&self.POLY_TIMESTAMP).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_API_KEY").unwrap(),
            HeaderValue::from_str(&self.POLY_API_KEY).unwrap(),
        );
        headers.insert(
            HeaderName::from_str("POLY_PASSPHRASE").unwrap(),
            HeaderValue::from_str(&self.POLY_PASSPHRASE).unwrap(),
        );

        headers
    }
}

pub fn create_level_2_headers(
    signer: &PolySigner,
    creds: &ApiCreds,
    request_args: &RequestArgs,
) -> Headers {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .as_secs()
        .to_string();

    let timestamp_headers = timestamp.clone();

    let hmac_sig = build_hmac_signature(
        &creds.api_secret,
        &timestamp,
        &request_args.method,
        &request_args.request_path,
        request_args.body,
    );
    let headers = Headers {
        POLY_ADDRESS: to_checksum(&signer.address(), None),
        POLY_SIGNATURE: hmac_sig,
        POLY_TIMESTAMP: timestamp_headers,
        POLY_API_KEY: creds.api_key.clone(),
        POLY_PASSPHRASE: creds.api_pass.clone(),
    };
    headers
}

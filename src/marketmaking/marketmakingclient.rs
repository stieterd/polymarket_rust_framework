use crate::clob_client::{client::ClobClient, clob_types::ApiCreds};
use crate::credentials::{
    ADDRESS, POLY_API_KEY, POLY_API_PASSPHRASE, POLY_API_SECRET, PRIVATE_KEY,
};
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    pub static ref CREDENTIALS: ApiCreds = ApiCreds {
        api_key: POLY_API_KEY.to_string(),
        api_secret: POLY_API_SECRET.to_string(),
        api_pass: POLY_API_PASSPHRASE.to_string(),
    };
    pub static ref CLIENT: Arc<ClobClient> = Arc::new(ClobClient::new(
        PRIVATE_KEY,
        CREDENTIALS.clone(),
        Some(2),
        Some(*ADDRESS)
    ));
}

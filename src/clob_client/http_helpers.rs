use lazy_static::lazy_static;
use reqwest::{
    header::{HeaderMap, HeaderName, HeaderValue},
    Client, Proxy, StatusCode,
};
use serde_json::Value;
use std::{error::Error, str::FromStr};

use crate::clob_client::clob_types::BalanceAllowanceParameters;

pub const GET: &str = "GET";
pub const POST: &str = "POST";
pub const DELETE: &str = "DELETE";
pub const PUT: &str = "PUT";

// Define a global, lazily-initialized reqwest Client
lazy_static! {
    static ref GLOBAL_CLIENT: Client = {
        Client::builder()
            .pool_idle_timeout(Some(std::time::Duration::from_secs(5000)))
            .pool_max_idle_per_host(20)
            // .proxy(Proxy::all("http://34.245.127.181:3128").unwrap())
            .danger_accept_invalid_certs(true)
            // .cookie_store(true)
            // .http2_prior_knowledge()
            // Optional: enable OS-level TCP keep-alive to prevent idle disconnections
            // .tcp_keepalive(Some(std::time::Duration::from_secs(300)))
            .timeout(std::time::Duration::from_secs(5)) // Customize as needed
            .tcp_nodelay(true)
            .tcp_keepalive(Some(std::time::Duration::from_secs(60)))
            .build()
            .expect("Failed to build reqwest client")
    };
}

// // Static, lazy-loaded proxy
// lazy_static! {
//     static ref PROXY: Proxy = Proxy::all("http://34.245.127.181:3128")
//         .expect("Failed to create proxy");
// }

pub fn overload_headers(method: &str, headers: Option<HeaderMap>) -> HeaderMap {
    let mut headers = headers.unwrap_or_else(HeaderMap::new);

    headers.insert(
        HeaderName::from_str("User-Agent").unwrap(),
        HeaderValue::from_str(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:135.0) Gecko/20100101 Firefox/135.0",
        )
        .unwrap(),
    );
    headers.insert(
        HeaderName::from_str("Accept-Encoding").unwrap(),
        HeaderValue::from_str("deflate").unwrap(),
    );
    headers.insert(
        HeaderName::from_str("Accept").unwrap(),
        HeaderValue::from_str("application/json, text/plain, */*").unwrap(),
    );
    headers.insert(
        HeaderName::from_str("Connection").unwrap(),
        HeaderValue::from_str("keep-alive").unwrap(),
    );
    headers.insert(
        HeaderName::from_str("Content-Type").unwrap(),
        HeaderValue::from_str("application/json").unwrap(),
    );
    headers.insert(
        HeaderName::from_str("Host").unwrap(),
        HeaderValue::from_str("clob.polymarket.com").unwrap(),
    );

    if method == GET {
        // headers.insert(HeaderName::from_str("Accept-Encoding").unwrap(), HeaderValue::from_str("gzip").unwrap());
    }

    headers
}

// Function to make the request
pub async fn request(
    endpoint: &str,
    method: &str,
    headers: Option<HeaderMap>,
    data: Option<&Value>,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    // Load headers with common fields
    let headers = overload_headers(method, headers);

    // Create a reqwest client
    let client = &*GLOBAL_CLIENT;

    // Determine the HTTP method and send the request accordingly
    let response = match method {
        GET => client.get(endpoint).headers(headers).send().await?,
        POST => {
            client
                .post(endpoint)
                .headers(headers)
                .json(&data)
                .send()
                .await?
        }
        DELETE => {
            let mut request = client.delete(endpoint).headers(headers);
            if let Some(data) = data {
                request = request.json(&data);
            }
            request.send().await?
            // client.delete(endpoint).headers(headers).json(&data).send().await?
        }

        PUT => {
            client
                .put(endpoint)
                .headers(headers)
                .json(&data)
                .send()
                .await?
        }
        _ => return Err("Unsupported HTTP method".into()),
    };

    // Check the status code and raise an error if it's not 200 OK
    if response.status() != StatusCode::OK {
        return Err(format!(
            "Request failed with status: {} {}",
            response.status(),
            response.text().await?
        )
        .into());
    }

    let text = response.text().await?; // Read the entire response body as text
                                       // Try to parse the response as JSON; if it fails, return the plain text
    match serde_json::from_str::<Value>(&text) {
        Ok(json) => Ok(json),
        Err(_) => Ok(Value::String(text)), // If parsing fails, return the raw text as a &str
    }
}

pub async fn post(
    endpoint: &str,
    headers: Option<HeaderMap>,
    data: Option<&Value>,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    request(endpoint, POST, headers, data).await
}

pub async fn get(
    endpoint: &str,
    headers: Option<HeaderMap>,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    request(endpoint, GET, headers, None).await
}

pub async fn delete(
    endpoint: &str,
    headers: Option<HeaderMap>,
    data: Option<&Value>,
) -> Result<Value, Box<dyn Error + Send + Sync>> {
    request(endpoint, DELETE, headers, data).await
}

pub fn build_query_params(url: &str, param: &str, val: &str) -> String {
    let mut url_with_params = String::from(url);

    // Get the last character of the URL, if it exists
    if let Some(last_char) = url_with_params.chars().last() {
        // If the last character is '?', append the param directly
        if last_char == '?' {
            url_with_params.push_str(&format!("{}={}", param, val));
        } else {
            // Otherwise, append "&" before the param
            url_with_params.push_str(&format!("&{}={}", param, val));
        }
    } else {
        // If the URL is empty, just append the param with '?'
        url_with_params.push_str(&format!("?{}={}", param, val));
    }

    url_with_params
}

/// Translates the Python `add_balance_allowance_params_to_url` into Rust.
pub fn add_balance_allowance_params_to_url(
    base_url: &str,
    params: Option<&BalanceAllowanceParameters>,
) -> String {
    let mut url = base_url.to_string();

    if let Some(params) = params {
        // Start the query string
        url.push('?');

        // Append each parameter if present
        if let Some(asset_type) = &params.asset_type {
            url = build_query_params(&url, "asset_type", &asset_type.to_string());
        }
        if let Some(token_id) = &params.token_id {
            url = build_query_params(&url, "token_id", token_id);
        }
        if let Some(signature_type) = &params.signature_type {
            url = build_query_params(&url, "signature_type", &signature_type.to_string());
        }
    }

    url
}

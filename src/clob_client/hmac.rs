use base64::engine::general_purpose::URL_SAFE;
use base64::Engine;
use hmac::{Hmac, Mac};
use serde_json::{to_string, Value};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn build_hmac_signature(
    secret: &str,
    timestamp: &str,
    method: &str,
    request_path: &str,
    body: Option<&Value>,
) -> String {
    let decoded_secret = URL_SAFE
        .decode(secret)
        .expect("Failed to decode base64 secret");

    // Build the message
    let mut message = format!("{}{}{}", timestamp, method, request_path);
    // If there is a body, append it (replace single quotes with double quotes)
    if let Some(body_value) = body {
        // Convert serde_json::Value to a String (JSON format)
        if let Ok(body_str) = to_string(&body_value) {
            // Perform the replace operation
            let formatted_body = body_str.replace("'", "\"");
            // Append the formatted body to the message
            message.push_str(&formatted_body);
        } else {
            // Handle the error in case of serialization failure
            message.push_str("Failed to serialize JSON body");
        }
    }

    // Create HMAC with the decoded secret
    let mut mac = HmacSha256::new_from_slice(&decoded_secret).expect("Failed to decode secret");

    // Input the message
    mac.update(message.as_bytes());

    // Compute the HMAC digest and encode it in base64
    let result = mac.finalize();
    let hmac_bytes = result.into_bytes();

    // Return the base64-encoded result
    URL_SAFE.encode(hmac_bytes)
}

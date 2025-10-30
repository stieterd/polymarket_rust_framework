use chrono::Utc;
use rand::Rng;
use serde_json::{json, Map, Value};

use super::builder::SignedOrder;

pub fn prepend_zx(mut in_str: String) -> String {
    /*
        Prepend 0x to the input string if it is missing
    */

    if in_str.chars().count() > 2 && in_str[..2].to_string() != "0x" {
        in_str = format!("0x{}", in_str);
    }
    in_str
}

pub fn generate_seed() -> u64 {
    // Get the current UTC timestamp as a floating-point number (seconds since epoch)
    let now = Utc::now();
    let timestamp = now.timestamp() as f64 + f64::from(now.timestamp_subsec_nanos()) * 1e-9;

    // Generate a random floating-point number between 0 and 1
    let mut rng = rand::thread_rng();
    let random_number: f64 = rng.gen();

    // Multiply the timestamp by the random number and round the result
    let seed = (timestamp * random_number).round() as u64;
    // seed = 1;
    seed
}

pub fn order_to_json(order: &SignedOrder, owner: &str, order_type: &str) -> Value {
    // Get the order as a HashMap<String, String>
    let order_dict = order.to_dict();

    // Create a mutable JSON map
    let mut order_json_map = Map::new();

    // Define the keys that need to be converted to integers
    let keys_to_convert_to_int = vec!["signatureType", "salt"]; // Replace with the actual keys

    for (k, v) in order_dict {
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
    json!({
        "order": order_value,
        "owner": owner,
        "orderType": order_type
    })
}

use reqwest::header::{
    ACCEPT, ACCEPT_ENCODING, ACCEPT_LANGUAGE, AUTHORIZATION, CONNECTION, CONTENT_TYPE, ORIGIN,
    USER_AGENT,
};
use serde_json::{json, Value};

pub async fn get_cash() -> i64 {
    let client = reqwest::Client::new();

    // Construct the payload
    let payload = json!([
        {
            "jsonrpc": "2.0",
            "id": 66,
            "method": "eth_call",
            "params": [
                {
                    "data": "0x70a082310000000000000000000000004f2ba33b080882c0f6b296f48cfd07b6c326f448",
                    "to": "0x2791bca1f2de4661ed88a30c99a7a9449aa84174"
                },
                "latest"
            ]
        }
    ]);

    let response = client
        .post("https://polygon-mainnet.g.alchemy.com/v2/")
        .header("Host", "polygon-mainnet.g.alchemy.com")
        .header(
            USER_AGENT,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:132.0) Gecko/20100101 Firefox/132.0",
        )
        .header(ACCEPT, "*/*")
        .header("Accept-Language", "en-US,en;q=0.5")
        // .header(ACCEPT_ENCODING, "gzip, deflate, br, zstd")
        .header(CONTENT_TYPE, "application/json")
        .header(AUTHORIZATION, "Bearer mewmTuTYExTSI0lZisD2szwmi35fZY-r")
        .header(CONNECTION, "keep-alive")
        .header("Origin", "https://polymarket.com")
        .json(&payload)
        .send()
        .await
        .unwrap();

    let body = response.text().await.unwrap();
    let response_json: Value = serde_json::from_str(&body).unwrap();

    // Access the result field
    if let Some(result_str) = response_json[0]["result"].as_str() {
        let result_int = u128::from_str_radix(&result_str[2..], 16).unwrap();
        (result_int / 1000000) as i64
    } else {
        eprintln!("Result not found");
        -1 as i64
    }
}

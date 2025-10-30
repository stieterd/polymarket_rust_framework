use std::{
    collections::HashMap,
    error::Error,
    sync::Arc,
    time::{Instant, SystemTime, UNIX_EPOCH},
    vec,
};

use ethers::abi::Hash;
use serde_json::Value;

use crate::{clob_client::clob_types::OrderArgs, marketmaking::marketmakingclient::CLIENT};
use std::collections::HashSet;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]

pub struct OpenOrder {
    pub price: u32,
    pub size: u32,
    pub time: Instant,
}

impl OpenOrder {
    // Constructor that captures the current system time
    pub fn new(price: u32, size: u32) -> Self {
        OpenOrder {
            price,
            size,
            time: Instant::now(), // <-- Capture system time
        }
    }
}

#[derive(Debug, Clone)]
pub struct OrderManager {
    pub open_bids: HashMap<String, OpenOrder>,
    pub open_asks: HashMap<String, OpenOrder>,
    pub global_order_ids: Arc<RwLock<HashSet<String>>>,
}

impl OrderManager {
    pub fn new(global_order_ids: Arc<RwLock<HashSet<String>>>) -> Self {
        Self {
            open_asks: HashMap::new(),
            open_bids: HashMap::new(),
            global_order_ids,
        }
    }

    pub async fn place_bid_taker(
        &mut self,
        asset_id: &str,
        price: u32,
        size: u32,
        tick_size: &str,
    ) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let task_start = Instant::now();
        let client = Arc::clone(&CLIENT);

        // convert price/size into f64
        let f_price = price as f64 / 1000.0;
        let f_size = size as f64 / 1000.0;

        // compute the “expire” timestamp
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards");
        let epoch_secs: usize = now.as_secs() as usize + 60 + 15;

        // turn both &str into owned Strings so we can move them into the blocking task
        let asset_owned = asset_id.to_string();
        let tick_size_owned = tick_size.to_string();
        let client_clone = Arc::clone(&client);

        // offload only the blocking “create_order” call
        let signed_order = tokio::task::spawn_blocking(move || {
            // build OrderArgs *inside* the blocking closure, using the owned String
            let order_args = OrderArgs::new(
                &asset_owned, // now AssetArgs has an owned &str reference into a String
                f_price,
                f_size,
                "BUY",
                None,
                None,
                Some(epoch_secs),
                None,
            );
            client_clone.create_order(&order_args, &tick_size_owned, true)
        })
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error + Send + Sync>)?; // propagate JoinError

        let duration = task_start.elapsed();
        println!("Time elapsed to sign order is {:?}", duration);

        // back to async for the HTTP POST
        let posted_order = client.post_taker_order(&signed_order, "GTD").await?;

        println!("Time elapsed to receive response is {:?}", duration);
        Ok(posted_order)
    }

    pub async fn place_bid(
        &mut self,
        asset_id: &str,
        price: u32,
        size: u32,
        tick_size: &str,
    ) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let client = Arc::clone(&CLIENT);

        let f_price = price as f64 / 1000.0;
        let f_size = size as f64 / 1000.0;

        let order_args = OrderArgs::new(asset_id, f_price, f_size, "BUY", None, None, None, None);

        if let Some(first_order) = self.open_bids.values().next() {
            if first_order.time.elapsed().as_secs() < 1 {
                eprintln!("Can not place bid because of double orders");
                return Err(Box::new(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Cannot place bid because of double orders",
                )));
            }
        }

        // Create and post the order
        let order = client.create_order(&order_args, &tick_size, true);
        let posted_order = client.post_order(&order, "GTC").await;

        match posted_order {
            Ok(ref value) => {
                // Extract the order ID as a String
                let order_id = value["orderID"]
                    .as_str()
                    .ok_or("orderID is not a string")?
                    .to_string();

                // Add the order ID to the global HashSet
                {
                    let mut global_ids = self.global_order_ids.write().await;
                    global_ids.insert(order_id.clone());
                }

                // if value["status"] != "live"{
                //     println!("Bugged orderbooks");
                //     let _ = client.cancel_all().await;
                //     std::process::exit(1);
                // }
                // let cancel_bids_response;
                if !self.open_bids.is_empty() {
                    match self.cancel_all_bids().await {
                        Ok(resp) => (println!("cancel: {:?}", resp)),
                        Err(e) => {
                            eprintln!("Failed to cancel bids: {:?}", e);
                            return Err(e); // Abort placing new order
                        }
                    }
                }

                let order = OpenOrder::new(price, size);
                self.open_bids.insert(order_id, order);
                Ok(value.clone())
            }
            Err(e) => {
                // Return the error encapsulated in a Box
                eprintln!("Error when placing quote");
                eprintln!("result {:?}", e);
                Err(e)
            }
        }
    }

    pub async fn cancel_all_bids(&mut self) -> Result<Value, Box<dyn Error + Send + Sync>> {
        let client = Arc::clone(&CLIENT);
        let vec_of_bids: Vec<&str> = self
            .open_bids
            .iter()
            .map(|entry| entry.0.as_str())
            .collect();
        let resp = client.cancel_orders(&vec_of_bids).await;
        self.open_bids.clear();
        resp
    }

    pub async fn place_ask() {}
}

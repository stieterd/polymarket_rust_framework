use ethers::prelude::*;
use ethers::signers::{LocalWallet, Signer};
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct PolySigner {
    private_key: LocalWallet,
    chain_id: u128,
}

impl PolySigner {
    pub fn new(private_key: &str, chain_id: u128) -> Self {
        let wallet = LocalWallet::from_str(private_key).expect("Invalid private key");

        Self {
            private_key: wallet,
            chain_id: 137,
        }
    }

    pub fn address(&self) -> Address {
        self.private_key.address()
    }

    pub fn get_chain_id(&self) -> u128 {
        self.chain_id
    }

    pub fn sign(&self, message_hash: &H256) -> String {
        let signature = self
            .private_key
            .sign_hash(*message_hash)
            .expect("Signing failed");

        // Convert the signature into a 65-byte array
        let sig_bytes: [u8; 65] = signature.into();

        // Encode the signature bytes to a hexadecimal string
        hex::encode(sig_bytes)
    }
}

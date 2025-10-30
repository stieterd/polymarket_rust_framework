use ethers::types::{Address, U256};

// // Define the ClobAuth struct
// pub struct ClobAuth {
//     pub address: Address,
//     pub timestamp: String,
//     pub nonce: U256,
//     pub message: String,
// }

// impl ClobAuth {
//     // Create a new instance of ClobAuth
//     pub fn new(address: H160, timestamp: &str, nonce: U256, message: &str) -> Self {
//         Self {
//             address: address,
//             timestamp: timestamp.to_string(),
//             nonce: nonce,
//             message: message.to_string(),
//         }
//     }

//     // pub fn signable_bytes(domain: EIP712Domain) -> Vec<u8> {
//     //     let mut result = vec![0x19, 0x01];
//     //     result.extend();
//     // }
// }

pub struct EIP712Domain<'a> {
    pub name: Option<&'a str>,
    pub version: Option<&'a str>,
    pub chainId: Option<U256>,
    pub verifyingContract: Option<Address>,
    // pub salt: Option<Vec<[u8; 4]>>
}

impl<'a> EIP712Domain<'a> {
    // Create a new instance
    pub fn new(
        name: Option<&'a str>,
        version: Option<&'a str>,
        chainId: Option<U256>,
        verifyingContract: Option<Address>,
    ) -> Self {
        Self {
            name,
            version,
            chainId,
            verifyingContract,
        }
    }
}

pub fn make_domain<'a>(
    name: Option<&'a str>,
    version: Option<&'a str>,
    chainId: Option<U256>,
    verifyingContract: Option<Address>,
) -> EIP712Domain<'a> {
    let mut eipStruct = EIP712Domain::new(None, None, None, None);

    if name.is_some() {
        eipStruct.name = name;
    }
    if version.is_some() {
        eipStruct.version = version;
    }
    if chainId.is_some() {
        eipStruct.chainId = chainId;
    }
    if verifyingContract.is_some() {
        eipStruct.verifyingContract = verifyingContract;
    }
    // if salt.is_some() {
    //     eipStruct.salt = salt;
    // }

    eipStruct
}

impl<'a> EIP712Domain<'a> {
    pub fn struct_hash(&self) -> [u8; 32] {
        use ethers::abi::{encode, Token};
        use ethers::utils::keccak256;

        // Construct the type string based on which fields are present
        let mut domain_type = "EIP712Domain(".to_string();
        let mut type_fields = Vec::new();
        let mut values = Vec::new();

        if let Some(name) = self.name {
            domain_type.push_str("string name,");
            type_fields.push(("name", "string"));
            values.push(Token::String(name.to_string()));
        }
        if let Some(version) = self.version {
            domain_type.push_str("string version,");
            type_fields.push(("version", "string"));
            values.push(Token::String(version.to_string()));
        }
        if let Some(chain_id) = self.chainId {
            domain_type.push_str("uint256 chainId,");
            type_fields.push(("chainId", "uint256"));
            values.push(Token::Uint(chain_id));
        }
        if let Some(verifying_contract) = self.verifyingContract {
            domain_type.push_str("address verifyingContract,");
            type_fields.push(("verifyingContract", "address"));
            values.push(Token::Address(verifying_contract));
        }

        // Remove the trailing comma and add closing parenthesis
        if domain_type.ends_with(',') {
            domain_type.pop();
        }
        domain_type.push(')');

        // Compute the type hash
        let type_hash = keccak256(domain_type.as_bytes());

        // Compute the encoded values
        let mut encoded_values = vec![Token::FixedBytes(type_hash.to_vec())];

        // For each field, compute its hash or value
        for (i, (_name, typ)) in type_fields.iter().enumerate() {
            match *typ {
                "string" => {
                    if let Token::String(ref s) = values[i] {
                        let hash = keccak256(s.as_bytes());
                        encoded_values.push(Token::FixedBytes(hash.to_vec()));
                    }
                }
                "uint256" => {
                    encoded_values.push(values[i].clone());
                }
                "address" => {
                    encoded_values.push(values[i].clone());
                }
                _ => {}
            }
        }

        // Encode the data
        let encoded = encode(&encoded_values);

        // Compute the struct hash
        let struct_hash = keccak256(&encoded);

        struct_hash
    }
}

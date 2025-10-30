use std::any::Any;
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::hash::{Hash, Hasher};

use serde_json::{json, Value as JsonValue};
use tiny_keccak::{Hasher as KeccakHasher, Keccak};

// Define the EIP712Type trait to represent different EIP712 types.

pub trait EIP712Type: fmt::Debug + Any {
    fn type_name(&self) -> &str;
    fn encode_value(&self) -> Vec<u8>;
    fn as_any(&self) -> &dyn Any;
}

// Implement EIP712Type for basic types.
impl EIP712Type for bool {
    fn type_name(&self) -> &str {
        "bool"
    }

    fn encode_value(&self) -> Vec<u8> {
        if *self {
            vec![0x01]
        } else {
            vec![0x00]
        }
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

impl EIP712Type for String {
    fn type_name(&self) -> &str {
        "string"
    }

    fn encode_value(&self) -> Vec<u8> {
        let mut keccak = Keccak::v256();
        keccak.update(self.as_bytes());
        let mut output = [0u8; 32];
        keccak.finalize(&mut output);
        output.to_vec()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// Implement EIP712Type for u64 (as uint256)
impl EIP712Type for u64 {
    fn type_name(&self) -> &str {
        "uint256"
    }

    fn encode_value(&self) -> Vec<u8> {
        let mut buf = [0u8; 32];
        buf[24..].copy_from_slice(&self.to_be_bytes());
        buf.to_vec()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

// EIP712Struct represents an EIP712 struct.
#[derive(Debug)]
pub struct EIP712Struct {
    pub type_name: String,
    pub values: HashMap<String, Box<dyn EIP712Type>>,
}

impl EIP712Struct {
    pub fn new(type_name: String, values: HashMap<String, Box<dyn EIP712Type>>) -> Self {
        EIP712Struct { type_name, values }
    }

    // Encode the struct's value.
    pub fn encode_value(&self) -> Vec<u8> {
        let mut encoded_values = Vec::new();
        for name in self.get_member_names() {
            let value = self.values.get(&name).expect("Value not found");
            if let Some(sub_struct) = value.as_any().downcast_ref::<EIP712Struct>() {
                // Nested struct: append its hash.
                encoded_values.extend_from_slice(&sub_struct.hash_struct());
            } else {
                // Basic type: encode value directly.
                encoded_values.extend_from_slice(&value.encode_value());
            }
        }
        encoded_values
    }

    // Get the type hash.
    pub fn type_hash(&self) -> [u8; 32] {
        let encoded_type = self.encode_type();
        keccak256(encoded_type.as_bytes())
    }

    // Hash the struct.
    pub fn hash_struct(&self) -> [u8; 32] {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&self.type_hash());
        bytes.extend_from_slice(&self.encode_value());
        keccak256(&bytes)
    }

    // Get the encoded type signature.
    pub fn encode_type(&self) -> String {
        let mut member_sigs = Vec::new();
        for name in self.get_member_names() {
            let typ = self.values.get(&name).expect("Type not found");
            if let Some(sub_struct) = typ.as_any().downcast_ref::<EIP712Struct>() {
                member_sigs.push(format!("{} {}", sub_struct.type_name, name));
            } else {
                member_sigs.push(format!("{} {}", typ.type_name(), name));
            }
        }
        let mut struct_sig = format!("{}({})", self.type_name, member_sigs.join(","));
        let mut reference_structs = HashSet::new();
        self.gather_reference_structs(&mut reference_structs);
        let mut sorted_structs: Vec<_> = reference_structs.into_iter().collect();
        sorted_structs.sort_by(|a, b| a.type_name.cmp(&b.type_name));
        for struct_type in sorted_structs {
            if struct_type.type_name != self.type_name {
                struct_sig.push_str(&struct_type.encode_type());
            }
        }
        struct_sig
    }

    // Gather reference structs used within this struct.
    fn gather_reference_structs<'a>(&'a self, struct_set: &mut HashSet<&'a EIP712Struct>) {
        for value in self.values.values() {
            if let Some(sub_struct) = value.as_any().downcast_ref::<EIP712Struct>() {
                if struct_set.insert(sub_struct) {
                    sub_struct.gather_reference_structs(struct_set);
                }
            }
        }
    }

    // Get member names in order.
    fn get_member_names(&self) -> Vec<String> {
        self.values.keys().cloned().collect()
    }

    // Convert to a message suitable for signing.
    pub fn to_message(&self, domain: &EIP712Struct) -> JsonValue {
        let mut types = serde_json::Map::new();
        let mut structs = HashSet::new();
        structs.insert(self);
        structs.insert(domain);
        self.gather_reference_structs(&mut structs);

        for struct_type in structs {
            let members = struct_type
                .get_member_names()
                .iter()
                .map(|name| {
                    let typ = struct_type.values.get(name).unwrap();
                    let type_name =
                        if let Some(sub_struct) = typ.as_any().downcast_ref::<EIP712Struct>() {
                            sub_struct.type_name.clone()
                        } else {
                            typ.type_name().to_string()
                        };
                    json!({ "name": name, "type": type_name })
                })
                .collect::<Vec<_>>();
            types.insert(struct_type.type_name.clone(), JsonValue::Array(members));
        }

        json!({
            "primaryType": self.type_name,
            "types": types,
            "domain": domain.to_data_dict(),
            "message": self.to_data_dict(),
        })
    }

    // Helper to convert struct values to a dictionary.
    pub fn to_data_dict(&self) -> JsonValue {
        let mut map = serde_json::Map::new();
        for (k, v) in &self.values {
            if let Some(sub_struct) = v.as_any().downcast_ref::<EIP712Struct>() {
                map.insert(k.clone(), sub_struct.to_data_dict());
            } else if let Some(string_value) = v.as_any().downcast_ref::<String>() {
                map.insert(k.clone(), JsonValue::String(string_value.clone()));
            } else if let Some(bool_value) = v.as_any().downcast_ref::<bool>() {
                map.insert(k.clone(), JsonValue::Bool(*bool_value));
            } else if let Some(uint_value) = v.as_any().downcast_ref::<u64>() {
                map.insert(k.clone(), JsonValue::Number((*uint_value).into()));
            } else {
                // Handle other types as needed.
            }
        }
        JsonValue::Object(map)
    }

    // Generate the signable bytes.
    pub fn signable_bytes(&self, domain: &EIP712Struct) -> Vec<u8> {
        let mut result = vec![0x19, 0x01];
        result.extend_from_slice(&domain.hash_struct());
        result.extend_from_slice(&self.hash_struct());
        result
    }
}

// Helper function to compute keccak256 hash.
fn keccak256(input: &[u8]) -> [u8; 32] {
    let mut keccak = Keccak::v256();
    keccak.update(input);
    let mut output = [0u8; 32];
    keccak.finalize(&mut output);
    output
}

// Implement PartialEq and Eq for EIP712Struct.
impl PartialEq for EIP712Struct {
    fn eq(&self, other: &Self) -> bool {
        self.type_name == other.type_name
            && self.encode_value() == other.encode_value()
            && self.encode_type() == other.encode_type()
    }
}

impl Eq for EIP712Struct {}

// Implement Hash for EIP712Struct.
impl Hash for EIP712Struct {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.type_name.hash(state);
        for (k, v) in &self.values {
            k.hash(state);
            // For hashing, you may need to define how to hash each value type.
            // This is a simplified example.
        }
    }
}

// Implement Index and IndexMut to access values like a dictionary.
use std::ops::{Index, IndexMut};

impl Index<&str> for EIP712Struct {
    type Output = Box<dyn EIP712Type>;

    fn index(&self, key: &str) -> &Self::Output {
        self.values.get(key).expect("Key not found")
    }
}

impl IndexMut<&str> for EIP712Struct {
    fn index_mut(&mut self, key: &str) -> &mut Self::Output {
        self.values.get_mut(key).expect("Key not found")
    }
}

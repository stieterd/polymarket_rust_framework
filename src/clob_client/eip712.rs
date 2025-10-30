use lazy_static::lazy_static;

use super::constants::POLYGON;

use super::clob_auth::EIP712Domain;

const CLOB_DOMAIN_NAME: &str = "ClobAuthDomain";
const CLOB_VERSION: &str = "1";
const MSG_TO_SIGN: &str = "This message attests that I control the given wallet";

lazy_static! {
    static ref CLOB_AUTH_DOMAIN: EIP712Domain<'static> = EIP712Domain::new(
        Some(CLOB_DOMAIN_NAME),
        Some(CLOB_VERSION),
        Some(ethers::types::U256::from(POLYGON)),  // This should be a constant as well
        None
    );
}

// fn get_clob_auth_domain() -> &'static EIP712Domain<'static> {
//     &CLOB_AUTH_DOMAIN
//     // let eip_domain = EIP712Domain::new(
//     //     Some(CLOB_DOMAIN_NAME),
//     //     Some(CLOB_VERSION),
//     //     Some(chain_id),
//     //     None
//     // );
//     // eip_domain
// }

#!/usr/bin/env python3
import os
import sys
import json
import argparse
import requests
import math
from eth_utils import keccak, to_bytes, to_checksum_address
from eth_account import Account
from eth_account.messages import encode_defunct
from web3 import Web3
from eth_abi import encode as abi_encode
import time 
import threading
import config
from eth_account.messages import encode_defunct

w3 = Web3()

PRIV = config.DERRY_POLYMARKET_PRIVATE_KEY
ACCOUNT_RELAY_ADDRESS = "0x49f0a389393b91cF809e281D067734646f1ACF90"
ACCOUNT_ADDRESS = config.DERRY_POLYMARKET_ADDRESS
with open("proxy_abi.json", 'r') as reader:
    PROXY_ABI = json.load(reader)

with open("poly_abi.json", 'r') as reader:
    POLY_ABI = json.load(reader)

POLY_CLAIM_CONTRACT_ADDRESS = "0xA238CBeb142c10Ef7Ad8442C6D1f9E89e07e7761"
POLY_CONTRACT_ADDRESS = "0x4D97DCd97eC945f40cF65F87097ACe5EA0476045"
EXCHANGE_COLLATERAL = "0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174"

# POLY_CONTRACT_ADDRESS = "0xd91E80cF2E7be2e162c6513ceD06f1dD0dA35296"

proxy_contract = w3.eth.contract(abi=PROXY_ABI)
poly_contract = w3.eth.contract(
    address=POLY_CONTRACT_ADDRESS,
    abi=POLY_ABI 
    )

poly_claim_contract = w3.eth.contract(
    address=POLY_CLAIM_CONTRACT_ADDRESS,
    abi=[{"inputs":[],"stateMutability":"nonpayable","type":"constructor"},{"inputs":[{"internalType":"bytes","name":"transactions","type":"bytes"}],"name":"multiSend","outputs":[],"stateMutability":"payable","type":"function"}]
)

def refresh_cookie() -> str:
    url = "https://gamma-api.polymarket.com/login"
    headers = {
        "Authorization": "Bearer WyIweDBhYzEzZGE2M2E3MGFlMTVhYjI3OGNjZTQ3YzZiNTEzMTU0YzYxZDhhYzcyMGNhOTkyMWVlNWE5YTdlZmQzMmM3MDIxYTgxNmEzODc5NmQ5MzA2YjdhNmM3NDE1OGU2MjkxY2Y1MWFjMWQ5YmFmYzhiYzBiNmJkZDFmZGMyYjhmMWIiLCJ7XCJpYXRcIjoxNzU0OTE0NjE1LFwiZXh0XCI6MTc1NTUxOTQxNSxcImlzc1wiOlwiZGlkOmV0aHI6MHhFODIzMmQ1Q0JBQkMwNjk1NjEyNDViQTA0ODk4NTFGMjhENjIzMzBkXCIsXCJzdWJcIjpcImN2aVBIVVE4SDhDZFZQMHJ1cEd1YnB5Y0xyQ19VNkNqLUpsbFlJLVFiQTg9XCIsXCJhdWRcIjpcIlh5ZU4tbERZeDZOZTk1OUxkVWZKSmVURXdWaGVQWG9mYlVEMGNZZ3RrZFE9XCIsXCJuYmZcIjoxNzU0OTE0NjE1LFwidGlkXCI6XCJlNzk2ZDkyMC02YmI4LTRmYjUtYjhmYi1lOWEzYTlhYjEwYjNcIixcImFkZFwiOlwiMHgzNTRhNjdlNTdjMWUzZGJiZmEyZmFjZjliNDlkNTYzYzg1NTVjYmM3NjA4MTQ5YWQ1NTMwMjUwODUyNGMwNTg1N2JhOTE2MGZlODQxMmU3OTYxNTkxNTRmNWNlNWEwMjUxMTVmYmIxMjRiNmQwMzhhZTU3NGVkYWM0NmNlMDI3NzFiXCJ9Il0=",
        # "Cookie": "AMP_4572e28e5c=JTdCJTIyZGV2aWNlSWQlMjIlM0ElMjJjYjU1MDFjMS01ZWU4LTQyZWEtOTFkYy05NmY2N2ZkYTM4MGYlMjIlMkMlMjJ1c2VySWQlMjIlM0ElMjIweDRmMkJBMzNiMDgwODgyYzBmNkIyOTZGNDhjZkQwN2I2QzMyNkY0NDglMjIlMkMlMjJzZXNzaW9uSWQlMjIlM0ExNzQ5NDAyMjUzNzQ4JTJDJTIyb3B0T3V0JTIyJTNBZmFsc2UlMkMlMjJsYXN0RXZlbnRUaW1lJTIyJTNBMTc0OTQxMTU1NjM3OSUyQyUyMmxhc3RFdmVudElkJTIyJTNBNjIxNTclN0Q=; AMP_MKTG_4572e28e5c=JTdCJTIycmVmZXJyZXIlMjIlM0ElMjJodHRwcyUzQSUyRiUyRmFjY291bnRzLmdvb2dsZS5jb20lMkYlMjIlMkMlMjJyZWZlcnJpbmdfZG9tYWluJTIyJTNBJTIyYWNjb3VudHM…2d6-b982c6967e82; intercom-session-zryw7npl=;",
        "User-Agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:138.0) Gecko/20100101 Firefox/138.0"
    }
    response = requests.get(url, headers=headers)
    return dict(response.cookies).get('polymarketsession')

def build_headers(session_cookie: str) -> dict:
    return {
        'Accept': "application/json, text/plain, */*",
        'Accept-Encoding': "gzip, deflate, br, zstd",
        'Accept-Language': "nl,en-US;q=0.7,en;q=0.3",
        'Connection': "keep-alive",
        'Cookie': f"polymarketsession={session_cookie}; polymarketauthtype=magic",
        'Host': "relayer-v2.polymarket.com",
        'Origin': "https://polymarket.com",
        'Priority': "u=0",
        'Sec-Fetch-Dest': "empty",
        'Sec-Fetch-Mode': "cors",
        'Sec-Fetch-Site': "same-site",
        'TE': "trailers",
        'User-Agent': "Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:138.0) Gecko/20100101 Firefox/138.0"
    }

HEADERS = {}  # will be filled by refresh_cookie()

def periodic_cookie_updater():
    global HEADERS
    while True:
        try:
            new_cookie = refresh_cookie()
            if new_cookie:
                HEADERS = build_headers(new_cookie)
                print(f"[INFO] Cookie refreshed: {new_cookie[:10]}... at {time.ctime()}")
            else:
                print("[WARN] Failed to get a new cookie")
        except Exception as e:
            print(f"[ERROR] Cookie refresh failed: {e}")
        time.sleep(1200)  # 20 minutes

# Start the updater thread at the beginning of the script
threading.Thread(target=periodic_cookie_updater, daemon=True).start()

def pad_uint256(value: int) -> bytes:
    return value.to_bytes(32, byteorder='big')

def address_bytes(addr: str) -> bytes:
    # ABI-packed address = 20 bytes, no padding
    return bytes.fromhex(to_checksum_address(addr)[2:])

def decode_proxy_hash(hex_str):
    # fn_obj, params = proxy_contract.decode_function_input(hex_str)
    # data = (params['calls'][0]['data'].hex())
    # print(params)
    # print(fn_obj.fn_name)
    
    fn_obj, merge_params = poly_contract.decode_function_input(hex_str)
    print(fn_obj.fn_name)
    return merge_params

def encode_merge_to_proxy_hash(_collateral_token:str, _conditionId: str, _amount:int):
    """
        format: {'_collateralToken': '0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174', '': [1, 2], '_conditionId': b'\xc3\x0c\xf2\x8f\x81\x08\xd5\xe8\x83\x03\x8c\x89\xaf\xea\xae\xadV\xc1\x970\xedr\x11$\x9c\xbf`\xc3_\xf7h\x90', '_amount': 5000000}
    """

    _conditionId = bytearray.fromhex(_conditionId[2:])
    params = {'_collateralToken': _collateral_token, '_parentCollectionId':b'\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00', 'partition':[1,2], '_conditionId': _conditionId, '_amount': _amount}

    args = [
        params['_collateralToken'],          # address
        params['_parentCollectionId'],                          # uint256[]
        params['_conditionId'],              # bytes32 (as bytes)
        params['partition'],
        params['_amount']                    # uint256
    ]

    new_data = poly_contract.encode_abi(
        "mergePositions",
        args=args
        )
    
    decoded = {
        'calls': [{
            'typeCode': 1,
            'to': POLY_CONTRACT_ADDRESS,
            'value': 0,
            'data': new_data
        }]
    }

    new_calldata = proxy_contract.encode_abi(
        'proxy',
        args=[decoded["calls"]]
    )

    # print("New proxy calldata:", new_calldata)
    return new_calldata

def encode_merge_to_proxy_hash_negrisk(_conditionId: str, _amount:int):
    """
        format: {'_collateralToken': '0x2791Bca1f2de4661ED88A30C99A7a9449Aa84174', '': [1, 2], '_conditionId': b'\xc3\x0c\xf2\x8f\x81\x08\xd5\xe8\x83\x03\x8c\x89\xaf\xea\xae\xadV\xc1\x970\xedr\x11$\x9c\xbf`\xc3_\xf7h\x90', '_amount': 5000000}
    """

    _conditionId = bytearray.fromhex(_conditionId[2:])
    params = {'_conditionId': _conditionId, '_amount': _amount}

    args = [
        # params['_collateralToken'],          # address
        # params[''],                          # uint256[]
        params['_conditionId'],              # bytes32 (as bytes)
        params['_amount']                    # uint256
    ]

    new_data = poly_contract.encode_abi(
        "mergePositions",
        args=args
        )
    
    decoded = {
        'calls': [{
            'typeCode': 1,
            'to': POLY_CONTRACT_ADDRESS,
            'value': 0,
            'data': new_data
        }]
    }

    new_calldata = proxy_contract.encode_abi(
        'proxy',
        args=[decoded["calls"]]
    )

    # print("New proxy calldata:", new_calldata)
    return new_calldata

def encode_convert_to_proxy_hash_negrisk(_market_id: str, index_set: int, _amount: int):
    """
    Build the proxy calldata for convertPositions, passing index_set as an integer
    so Web3.py will ABI‑encode it as a big‑endian uint256.
    """
    # sanity check: index_set must be an integer
    if not isinstance(index_set, int):
        raise TypeError(f"index_set must be int, got {type(index_set)}")

    # Convert the hex string market ID to 32 bytes
    market_id_bytes = bytes.fromhex(_market_id[2:])

    # Let Web3.py handle the padding of uint256 fields by passing Python ints
    new_data = poly_contract.encode_abi(
        "convertPositions",
        args=[market_id_bytes, index_set, _amount]
    )

    # Wrap in proxy call
    calls = [{
        'typeCode': 1,
        'to': POLY_CONTRACT_ADDRESS,
        'value': 0,
        'data': new_data
    }]

    # Build final proxy calldata
    new_calldata = proxy_contract.encode_abi(
        'proxy',
        args=[calls]
    )
    return new_calldata



def get_nonce_relay()->(int,str):
    nonce_url = f"https://relayer-v2.polymarket.com/relay-payload?address={ACCOUNT_RELAY_ADDRESS}&type=SAFE"

    resp = requests.get(nonce_url, headers=HEADERS)
    
    if resp.status_code == 200:
        print(resp.status_code)
        relay = resp.json()["address"]
        nonce = int(resp.json()["nonce"])
        return (nonce, relay)
    else:
        print(resp.text)
        exit()

# --- EIP-712 helpers mirroring the JS hashTypedData path ---
def hash_safe_typed_data(signable_data: dict) -> bytes:
    """Compute keccak(0x1901 || domainSeparator || structHash) for SafeTx.
    Expects signable_data with keys: types, domain, primaryType=="SafeTx", message.
    Domain is {chainId:uint256, verifyingContract:address}."""
    types = signable_data["types"]
    domain = signable_data["domain"]
    message = signable_data["message"]

    # Type hashes must reflect the exact field order used in JS/types
    # EIP712Domain(chainId,address verifyingContract)
    DOMAIN_TYPEHASH = keccak(text="EIP712Domain(uint256 chainId,address verifyingContract)")
    # SafeTx(...) in the exact order from your JS
    SAFE_TX_TYPEHASH = keccak(text=(
        "SafeTx("
        "address to,uint256 value,bytes data,uint8 operation,"
        "uint256 safeTxGas,uint256 baseGas,uint256 gasPrice,"
        "address gasToken,address refundReceiver,uint256 nonce)"
    ))

    # bytes -> keccak(bytes)
    data_hash = keccak(message["data"])  

    # structHash of SafeTx
    struct_encoded = abi_encode(
        [
            "bytes32",
            "address", "uint256", "bytes32", "uint8",
            "uint256", "uint256", "uint256",
            "address", "address", "uint256",
        ],
        [
            SAFE_TX_TYPEHASH,
            message["to"], message["value"], data_hash, message["operation"],
            message["safeTxGas"], message["baseGas"], message["gasPrice"],
            message["gasToken"], message["refundReceiver"], message["nonce"],
        ],
    )
    struct_hash = keccak(struct_encoded)

    # domainSeparator
    domain_encoded = abi_encode(
        ["bytes32", "uint256", "address"],
        [DOMAIN_TYPEHASH, domain["chainId"], domain["verifyingContract"]],
    )
    domain_separator = keccak(domain_encoded)

    # Final digest: keccak(0x1901 || domain || struct)
    return keccak(b"\x19\x01" + domain_separator + struct_hash)


# ----------------------------------------------------------------SUGGESTIE VAN GEMINI
# def merge_tokens(
#     conditionId: bytes,
#     amount: int,
#     partition=(1, 2),
#     parentCollectionId=b"\x00" * 32
# ):
#     """
#     Send mergePositions via Polymarket relayer as a SafeTx meta-transaction (gasless).
#     This version correctly uses EIP-712 signing with modern eth-account.
#     """
#     # 1. Get Nonce (Your existing code is correct)
#     nonce, _ = get_nonce_relay()

#     # 2. Prepare accounts and contract instances (Your existing code is correct)
#     acct = Account.from_key(PRIV)
#     collateralToken = EXCHANGE_COLLATERAL
#     proxy_wallet_address = to_checksum_address(ACCOUNT_ADDRESS) 
    
#     # 3. Encode the underlying call to `mergePositions` (Your existing code is correct)
#     encoded_merge_call = poly_contract.encode_abi(
#         "mergePositions",
#         args=[
#             to_checksum_address(collateralToken),
#             parentCollectionId,
#             conditionId,
#             list(partition),
#             int(amount),
#         ],
#     )

#     # 4. Define the EIP-712 structured data for the SafeTx
#     domain = {
#         "chainId": 137,
#         "verifyingContract": proxy_wallet_address 
#     }

#     types = {
#         "SafeTx": [
#             {"name": "to", "type": "address"},
#             {"name": "value", "type": "uint256"},
#             {"name": "data", "type": "bytes"},
#             {"name": "operation", "type": "uint8"},
#             {"name": "safeTxGas", "type": "uint256"},
#             {"name": "baseGas", "type": "uint256"},
#             {"name": "gasPrice", "type": "uint256"},
#             {"name": "gasToken", "type": "address"},
#             {"name": "refundReceiver", "type": "address"},
#             {"name": "nonce", "type": "uint256"},
#         ]
#     }

#     # 5. Create the message with the actual transaction values
#     message = {
#         "to": to_checksum_address(POLY_CONTRACT_ADDRESS),
#         "value": 0,
#         "data": to_bytes(hexstr=encoded_merge_call),
#         "operation": 0,
#         "safeTxGas": 0,
#         "baseGas": 0,
#         "gasPrice": 0,
#         "gasToken": "0x0000000000000000000000000000000000000000",
#         "refundReceiver": "0x0000000000000000000000000000000000000000",
#         "nonce": nonce
#     }

#     signable_data = {
#         "types": types,
#         "domain": domain,
#         "primaryType": "SafeTx",
#         "message": message,
#     }

#     # 6. Sign the structured data using EIP-712
#     signed_message = acct.sign_typed_data(full_message=signable_data)
#     sig = signed_message.signature.hex()

#     # 7. Build the final payload for the relayer API
#     payload = {
#         "from": proxy_wallet_address,
#         "to": to_checksum_address(POLY_CONTRACT_ADDRESS),
#         "proxyWallet": proxy_wallet_address,
#         "data": encoded_merge_call,
#         "nonce": str(nonce),
#         "signature": sig,
#         "signatureParams": {
#             "gasPrice": "0",
#             "operation": "0",
#             "safeTxnGas": "0",
#             "baseGas": "0",
#             "gasToken": "0x0000000000000000000000000000000000000000",
#             "refundReceiver": "0x0000000000000000000000000000000000000000",
#         },
#         "type": "SAFE",
#     }
    
#     # 8. POST to the relayer
#     print("[INFO] Submitting SafeTx to relayer...")
#     r = requests.post(
#         "https://relayer-v2.polymarket.com/submit",
#         json=payload,
#         timeout=10,
#         headers=HEADERS
#     )
#     return r.json()
# ----------------------------------------------SUGGESTIE VAN GEMINI
def pack_signature_uint256_uint256_uint8(sig_hex: str, v_mode: str = "keep") -> bytes:
    """
    abi.encodePacked(uint256 r, uint256 s, uint8 v)
    v_mode:
      - "keep": leave v as-is (27/28 or 0/1)
      - "01":   normalize to 0/1    (27->0, 28->1)
      - "27":   normalize to 27/28  (0->27, 1->28)
      - "bump4": add 4 to 27/28     (27->31, 28->32)  <-- matches your ...1f case
      - "force31": set v=31
    """
    h = sig_hex[2:] if sig_hex.startswith("0x") else sig_hex
    if len(h) != 130:
        raise ValueError(f"Expected 65-byte signature (130 hex chars), got {len(h)}")
    sig = bytes.fromhex(h)

    r_bytes = sig[0:32]
    s_bytes = sig[32:64]
    v = sig[64]

    if v_mode == "01":
        if v in (27, 28): v -= 27
    elif v_mode == "27":
        if v in (0, 1): v += 27
    elif v_mode == "bump4":
        if v in (27, 28): v += 4
    elif v_mode == "force31":
        v = 31
    # else "keep"

    return int.from_bytes(r_bytes, "big").to_bytes(32, "big") + \
           int.from_bytes(s_bytes, "big").to_bytes(32, "big") + \
           bytes([v])
def merge_tokens(
    conditionId: bytes,
    amount: int,
    partition=(1, 2),
    parentCollectionId=b"\x00" * 32
):
    """
    Send mergePositions via Polymarket relayer (gasless).
    Uses poly_contract (your Conditional Tokens contract instance).
    The tokens must be in PROXY_WALLET.
    """
    nonce, relay = get_nonce_relay()

    collateralToken = EXCHANGE_COLLATERAL
    acct = Account.from_key(PRIV)

    frm         = (ACCOUNT_RELAY_ADDRESS)               # use relayer-provided sender
    to          = (POLY_CONTRACT_ADDRESS)                  # SAFE flow: target contract here
    proxyWallet = (ACCOUNT_ADDRESS)                        # your Safe / proxy wallet address

    # Encode mergePositions call data using your existing contract instance
    encoded = poly_contract.encode_abi(
        "mergePositions",
        args=[
            Web3.to_checksum_address(collateralToken),
            parentCollectionId,
            conditionId,
            list(partition),
            int(amount),
        ],
    )

    # Relay parameters
    transactionFee = 0
    gasPrice       = 0
    gasLimit       = 269151

    sig_params = {
        "gasPrice":      str(0),
        "operation":     str(0),
        "safeTxnGas":    str(0),  # required by relayer schema
        "baseGas":       str(0),
        "gasToken":      ("0x0000000000000000000000000000000000000000"),
        "refundReceiver": ("0x0000000000000000000000000000000000000000"),
    }

    # --- EIP-712 domain and types ---
    POLYGON_CHAIN_ID = 137
    safe_address = proxyWallet
    domain = {
        "chainId": POLYGON_CHAIN_ID,
        "verifyingContract": safe_address,
    }
    types = {
        "EIP712Domain": [
            {"name": "chainId", "type": "uint256"},
            {"name": "verifyingContract", "type": "address"},
        ],
        "SafeTx": [
            {"name": "to", "type": "address"},
            {"name": "value", "type": "uint256"},
            {"name": "data", "type": "bytes"},
            {"name": "operation", "type": "uint8"},
            {"name": "safeTxGas", "type": "uint256"},
            {"name": "baseGas", "type": "uint256"},
            {"name": "gasPrice", "type": "uint256"},
            {"name": "gasToken", "type": "address"},
            {"name": "refundReceiver", "type": "address"},
            {"name": "nonce", "type": "uint256"},
        ],
    }

    message = {
        "to": (POLY_CONTRACT_ADDRESS),
        "value": 0,
        "data": to_bytes(hexstr=encoded),
        "operation": 0,
        "safeTxGas": 0,
        "baseGas": 0,
        "gasPrice": 0,
        "gasToken": "0x0000000000000000000000000000000000000000",
        "refundReceiver": "0x0000000000000000000000000000000000000000",
        "nonce": nonce,
    }

    print(message)

    signable_data = {
        "types": types,
        "domain": domain,
        "primaryType": "SafeTx",
        "message": message,
    }

    # JS-style EIP-712 digest (same as the l(e) function you shared)
    digest = hash_safe_typed_data(signable_data)
    print("[EIP712 digest]", "0x" + digest.hex())

    msg = encode_defunct(digest)
    signed = acct.sign_message(msg)
    sig_hex = signed.signature.hex()

    print("Signature:", signed.signature.hex())

    packed = pack_signature_uint256_uint256_uint8(sig_hex, v_mode="bump4")
    print("Packed ", packed.hex() )

    payload = {
        "from":        to_checksum_address(frm),
        "to":          to_checksum_address(to),
        "proxyWallet": to_checksum_address(proxyWallet),
        "data":        encoded,
        "nonce":       str(nonce),
        "signature":   "0x" + packed.hex(),
        "signatureParams": sig_params,
        "type": "SAFE",
    }

    r = requests.post(
        "https://relayer-v2.polymarket.com/submit",
        json=payload,
        timeout=10,
        headers=HEADERS
    )
    return r.json()

def positions_to_index_set(positions):
    """
    Given a list of non-negative integer positions, return the uint256 indexSet
    bitmask where bit i is set if i is in positions.
    """
    index_set = 0
    for pos in positions:
        if pos < 0:
            raise ValueError("Positions must be non-negative")
        index_set |= (1 << pos)
    return index_set

def encode_uint256_big_endian(value: int) -> bytes:
    """
    ABI-encode a uint256 as a 32-byte, big-endian word.
    """
    if value < 0 or value >= 1 << 256:
        raise ValueError("Value out of range for uint256")
    return value.to_bytes(32, byteorder="big")

def invert_mask(mask: int, question_count: int) -> int:
    """
    Return the bitwise complement of `mask` within the low `question_count` bits.
    """
    # all_ones has the low `question_count` bits set to 1
    all_ones = (1 << question_count) - 1
    return all_ones ^ mask


def positions_to_index_set_complement(positions: list[int], question_count: int) -> int:
    """
    Given a list of 'yes' position indices, return the indexSet for the 'no' positions
    by inverting within the range of question_count.
    """
    yes_mask = positions_to_index_set(positions)
    return invert_mask(yes_mask, question_count)

def convert_tokens_negrisk(marketId: str, indexSet: int, amount: int):
    """
    conditionId: hexadecimal representation of the condition of the tokens we want to merge
    amount: integer representation of amount of tokens we want to merge multiplied by 10^6
    """

    amount = int(math.floor(amount * 1000000))

    nonce, relay = get_nonce_relay()

    acct = Account.from_key(PRIV)

    # --- your static fields ---
    frm         = ACCOUNT_RELAY_ADDRESS
    to          = "0xaB45c5A4B0c941a2F231C04C3f49182e1A254052"
    proxyWallet = ACCOUNT_ADDRESS

    encoded = encode_convert_to_proxy_hash_negrisk(marketId, indexSet, amount)
    print(encoded)

    transactionFee = 0
    gasPrice       = 0
    gasLimit       = 6237523
    # --------------------------------

    packed = b"rlx:" \
        + address_bytes(frm) \
        + address_bytes(to) \
        + bytes.fromhex(encoded[2:]) \
        + pad_uint256(transactionFee) \
        + pad_uint256(gasPrice) \
        + pad_uint256(gasLimit) \
        + pad_uint256(nonce) \
        + address_bytes("0xD216153c06E857cD7f72665E0aF1d7D82172F494")

    # 2) Append relay at the end and keccak
    hb = keccak(packed + address_bytes(relay))

    # 3) Wrap with the standard "\x19Ethereum Signed Message:\n32" prefix
    msg = encode_defunct(primitive=hb)

    # 4) Sign
    sig = "0x" + acct.sign_message(msg).signature.hex()

    # 5) Build payload
    payload = {
        "from":           frm,
        "to":             to,
        "proxyWallet":    proxyWallet,
        "data":           encoded,
        "nonce":          str(nonce),
        "signature":      sig,
        "signatureParams": {
            "relayerFee": str(transactionFee),
            "gasPrice":       str(gasPrice),
            "gasLimit":       str(gasLimit),
            "relayHub":       "0xD216153c06E857cD7f72665E0aF1d7D82172F494",
            "relay":          relay,
        },
        "type": "PROXY",
    }



    print(payload)
    # 6) POST to the relayer
    r = requests.post("https://relayer-v2.polymarket.com/submit",
                      json=payload, timeout=10, headers=HEADERS)
    return r.json()

def main():

    data = "0x34ee979100000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000010000000000000000000000004d97dcd97ec945f40cf65f87097ace5ea04760450000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000008000000000000000000000000000000000000000000000000000000000000001049e7212ad0000000000000000000000002791bca1f2de4661ed88a30c99a7a9449aa8417400000000000000000000000000000000000000000000000000000000000000005626a8fdffcd2db7c93fc3039f0e21a4b641e222fe19e91457a9ec5d2c34845400000000000000000000000000000000000000000000000000000000000000a00000000000000000000000000000000000000000000000000000000001312d0000000000000000000000000000000000000000000000000000000000000000020000000000000000000000000000000000000000000000000000000000000001000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000"
    print(decode_proxy_hash(data)['conditionId'].hex())


if __name__ == "__main__":
    # print(convert_tokens_negrisk("0x6df9c389b900084450acb3df5777dcde9ff74f6a1532e9d78a715b5ee83dda00", positions_to_index_set([0, 2, 8, 18]), 5))
    # pass
    # print(positions_to_index_set([8, 10, 16, 12, 5, 14, 9, 7, 15, 13, 6, 11]))
    print(decode_proxy_hash("0x8d80ff0a000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000003ab004d97dcd97ec945f40cf65f87097ace5ea0476045000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e401b7037c0000000000000000000000002791bca1f2de4661ed88a30c99a7a9449aa841740000000000000000000000000000000000000000000000000000000000000000e906cb626a4f0a8f5b84de6e900ec4b1138f73e8382decc1dadffb1ca09126270000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002004d97dcd97ec945f40cf65f87097ace5ea0476045000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e401b7037c0000000000000000000000002791bca1f2de4661ed88a30c99a7a9449aa8417400000000000000000000000000000000000000000000000000000000000000006eac23ab1f4d4bdceea8af479bb145e4cf0ea9bda1d47e89bcde91ef365a475f0000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002004d97dcd97ec945f40cf65f87097ace5ea0476045000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000e401b7037c0000000000000000000000002791bca1f2de4661ed88a30c99a7a9449aa841740000000000000000000000000000000000000000000000000000000000000000d118e6803fe787e20d8566220cdaa6a41eee6218566f6abde8bb7c02c0d0a5830000000000000000000000000000000000000000000000000000000000000080000000000000000000000000000000000000000000000000000000000000000200000000000000000000000000000000000000000000000000000000000000010000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000"))
    # main()
    time.sleep(3)
    
    # print(merge_tokens("0x8430120f9cd3b78c39b3e9a657e31cd37eede284f3a016d0e611e8aebc41adad", 1_000_000))
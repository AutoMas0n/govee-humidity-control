#!/usr/bin/env python3
"""
Govee H5080 BLE Protocol Implementation
Based on reverse-engineered Govee Home app APK
"""
import struct, os, sys
from Crypto.Cipher import AES
from Crypto.Random import get_random_bytes

# ============== DERIVED KEYS ==============
# KEY_COMMUNICATION: AES-ECB decrypt(app_communication, key=app_session)
KEY_COMM = b"MakingLifeSmarte"  # 16 bytes
# KEY_COMMUNICATION_X: AES-ECB decrypt(app_y_com, key=app_x_name)
KEY_X = bytes.fromhex("FC03783C7C42CB83E202A1643648AFF6")  # 16 bytes  
# KEY_COMMUNICATION_Y: AES-ECB decrypt(app_x_com, key=app_y_name)
KEY_Y = bytes.fromhex("AE028B630BAE6ECC4BFF1B249E22F955")  # 16 bytes

AES_GCM_TAG_LEN = 12  # 96-bit tag (default)
AES_GCM_IV_LEN = 12

def xor_checksum(data):
    """XOR checksum of first 19 bytes."""
    return bytes([functools.reduce(lambda a, b: a ^ b, data[:19])])

import functools

# ============== V1 AES-ECB + RC4 Frame ==============
def build_v1_frame(cmd, sub, data=b''):
    """Build a 20-byte V1 plaintext frame."""
    frame = bytearray(20)
    frame[0] = cmd
    frame[1] = sub
    if data:
        frame[2:2+len(data)] = data
    # Fill bytes 2..18 with random if not filled
    for i in range(len(data) + 2, 19):
        frame[i] = os.urandom(1)[0]
    # XOR checksum
    cs = 0
    for i in range(19):
        cs ^= frame[i]
    frame[19] = cs
    return bytes(frame)

def rc4_encrypt(data, key):
    """RC4 PRNG (Safe.m1020g). Symmetric."""
    S = list(range(256))
    j = 0
    for i in range(256):
        j = (j + S[i] + key[i % len(key)]) & 0xFF
        S[i], S[j] = S[j], S[i]
    i = j = 0
    out = bytearray(len(data))
    for n in range(len(data)):
        i = (i + 1) & 0xFF
        j = (j + S[i]) & 0xFF
        S[i], S[j] = S[j], S[i]
        out[n] = data[n] ^ S[(S[i] + S[j]) & 0xFF]
    return bytes(out)

def v1_encrypt(frame, key=KEY_COMM):
    """V1: AES-ECB(first 16 bytes) + RC4(last 4 bytes)."""
    assert len(frame) == 20
    cipher = AES.new(key, AES.MODE_ECB)
    enc = cipher.encrypt(frame[:16])
    rc4_out = rc4_encrypt(frame[16:20], key)
    return enc + rc4_out

def v1_decrypt(payload, key=KEY_COMM):
    """V1: decrypt 20-byte payload."""
    assert len(payload) == 20
    cipher = AES.new(key, AES.MODE_ECB)
    dec = cipher.decrypt(payload[:16])
    rc4_out = rc4_encrypt(payload[16:20], key)
    return dec + rc4_out

def v1_verify(frame):
    """Verify XOR checksum at byte 19."""
    cs = functools.reduce(lambda a, b: a ^ b, frame[:19])
    return cs == frame[19]

# ============== V2 AES-GCM Frame ==============
def gcm_encrypt(plaintext, key, iv, aad=b''):
    """AES-128-GCM encrypt. Returns iv(12) + ciphertext (with tag appended)."""
    cipher = AES.new(key, AES.MODE_GCM, nonce=iv, mac_len=AES_GCM_TAG_LEN)
    if aad:
        cipher.update(aad)
    ct, tag = cipher.encrypt_and_digest(plaintext)
    return iv + ct + tag

def gcm_decrypt(ciphertext, key, iv, aad=b''):
    """AES-128-GCM decrypt. iv=12 bytes, ciphertext includes tag."""
    tag = ciphertext[-AES_GCM_TAG_LEN:]
    ct = ciphertext[:-AES_GCM_TAG_LEN]
    cipher = AES.new(key, AES.MODE_GCM, nonce=iv, mac_len=AES_GCM_TAG_LEN)
    if aad:
        cipher.update(aad)
    return cipher.decrypt_and_verify(ct, tag)

# ============== Session Key Exchange ==============
def make_v1_session_request():
    """Build the session request payload (first write, encrypted with KEY_COMM)."""
    frame = build_v1_frame(0xE7, 0x01, data=get_random_bytes(16))
    return v1_encrypt(frame, KEY_COMM)

def make_v1_session_confirm():
    """Build the session confirm (second write, encrypted with KEY_COMM)."""
    frame = build_v1_frame(0xE7, 0x02, data=get_random_bytes(16))
    return v1_encrypt(frame, KEY_COMM)

def extract_session_key_from_response(response_payload):
    """Extract session key from device response (decrypted with KEY_COMM)."""
    dec = v1_decrypt(response_payload, KEY_COMM)
    if not v1_verify(dec) or dec[0] != 0xE7 or dec[1] != 0x01:
        return None
    # Session key = bytes[2..17] (16 bytes)
    return bytes(dec[2:18])

# ============== V2 Session Key Exchange ==============
def make_v2_session_request(iv_key):
    """Build V2 session request using KEY_X AES-GCM."""
    # Build frame: [0x00, 0x19, 0x00, 0x02]
    frame_header = bytes([0x00, 0x19, 0x00, 0x02])
    # Random 12-byte nonce for GCM
    gcm_nonce = get_random_bytes(12)
    # AAD: header(4) + nonce(12) + tag_len(1)
    aad = frame_header + gcm_nonce + bytes([AES_GCM_TAG_LEN])
    # Encrypt iv_key (8 bytes) with KEY_X
    result = gcm_encrypt(iv_key, KEY_X, gcm_nonce, aad)
    # result = nonce(12) + ct(8) + tag(12) = 32 bytes
    encrypted_data = result[12:24]  # ct(8) 
    tag = result[24:]  # tag(12)
    # Build packet
    cmd = frame_header + encrypted_data + bytes([AES_GCM_TAG_LEN]) + tag
    return cmd  # 4 + 12 + 1 + 12 = 29 bytes, needs splitting

def derive_device_key(device_response_data, key=KEY_X):
    """Derive device key from V2 session response (m1005v logic)."""
    # device_response_data = 19 bytes decrypt result
    # [0..7] = session iv (8 bytes)
    # [8..12] = 5 bytes 
    # [13..18] = 6 bytes
    session_iv = device_response_data[:8]
    key_seed = device_response_data[8:13] + device_response_data[13:19]
    # Pad to 16 bytes (already 11 bytes, pad with zeros to 16)
    padded = key_seed + b'\x00' * (16 - len(key_seed))
    # AES-ECB encrypt with KEY_X
    cipher = AES.new(key, AES.MODE_ECB)
    device_key = cipher.encrypt(padded)  # No padding needed (ECB with NoPadding)
    return session_iv, device_key

# ============== Command Encryption ==============
def encrypt_v2_command(cmd_data, iv_key, device_key, counter=1):
    """Encrypt a command using V2 AES-GCM."""
    iv = iv_key + struct.pack('>I', counter)  # 8 + 4 = 12 bytes
    # AAD: [0xE7, 0x1A, 0x02] + counter(4)
    aad = bytes([0xE7, 0x1A, 0x02]) + struct.pack('>I', counter)
    result = gcm_encrypt(cmd_data, device_key, iv, aad)
    # result = iv(12) + ct + tag(12)
    ct = result[12:-AES_GCM_TAG_LEN]  # encrypted data without tag
    tag = result[-AES_GCM_TAG_LEN:]
    # Build BLE packet: counter(4) + ct + tag
    return struct.pack('>I', counter) + ct + tag

# ============== Test ==============
if __name__ == '__main__':
    print("Testing V1 encryption/decryption...\n")
    
    # Test V1 frame
    frame = build_v1_frame(0xE7, 0x01, get_random_bytes(16))
    print(f"Frame plain: {frame.hex()}")
    print(f"  cmd={frame[0]:02x} sub={frame[1]:02x} checksum_ok={v1_verify(frame)}")
    
    enc = v1_encrypt(frame, KEY_COMM)
    dec = v1_decrypt(enc, KEY_COMM)
    print(f"  enc={enc.hex()}")
    print(f"  dec={dec.hex()}")
    print(f"  match={frame == dec}")
    
    # Decrypt captured payloads
    print("\nDecrypting captured session A payloads with KEY_COMM:")
    captured = [
        "7f2ca83487a8fce37228eb9bbfd5e67652402360",
        "8b11b63d6aa6df61d5f6db717d8b43dbcb15faa3",
    ]
    for p in captured:
        data = bytes.fromhex(p)
        dec = v1_decrypt(data, KEY_COMM)
        print(f"  {p}")
        print(f"    → {dec.hex()} cmd={dec[0]:02x} sub={dec[1]:02x} checksum_ok={v1_verify(dec)}")

    print("\nAll keys:")
    print(f"  KEY_COMM = {KEY_COMM.hex()}")
    print(f"  KEY_X    = {KEY_X.hex()}")
    print(f"  KEY_Y    = {KEY_Y.hex()}")
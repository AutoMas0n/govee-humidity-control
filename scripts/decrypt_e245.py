from Cryptodome.Cipher import AES

KEY_COMM = bytes.fromhex("4d616b696e674c696665536d61727465")

def rc4(data, key):
    S = list(range(256)); j = 0
    for i in range(256): j = (j + S[i] + key[i % len(key)]) & 255; S[i], S[j] = S[j], S[i]
    i=j=0
    out=bytearray(len(data))
    for n in range(len(data)):
        i=(i+1)&255; j=(j+S[i])&255; S[i],S[j]=S[j],S[i]
        out[n]=data[n]^S[(S[i]+S[j])&255]
    return bytes(out)

def decrypt(payload, key):
    return AES.new(key, AES.MODE_ECB).decrypt(payload[:16]) + rc4(payload[16:20], key)

def verify(f):
    cs=0
    for b in f[:19]: cs^=b
    return cs==f[19]

# All handle 0x0011 writes from the capture
writes = [
    bytes.fromhex("494c38efde5564bba300cdb592f0702a59297125"),
    bytes.fromhex("523034ec38a3abd70ecccbd44f5c0766e51bca42"),
    bytes.fromhex("434acca1a650320b95703bb06546f21fb195c18c"),
    bytes.fromhex("a42dcd1c76e232c1bce137197c448d7eb195c1eb"),
    bytes.fromhex("270e639b87fd4080980bdef5dca1e15eb195c1de"),
    bytes.fromhex("dd538d5b11c003c28098a4e1269f97a6b195c16f"),
    bytes.fromhex("e54f8de488214590e3be90ede31a3f72b195c16e"),
    bytes.fromhex("e73c3eab8d9e9adfc8bfa9111262173cb195c1cd"),
]
print("=== Session 1: Decrypt with KEY_COMM ===")
for i, data in enumerate(writes):
    d = decrypt(data, KEY_COMM)
    print(f"  {i}: {d.hex()} v={verify(d)}")
    if d[0]==0xE7 and verify(d):
        if d[1]==0x01:
            sk = bytes(d[2:18])
            print(f"    -> E7 01 handshake, SK={sk.hex()}")
        elif d[1]==0x02:
            print(f"    -> E7 02 confirm")

print("\n=== Decrypt remaining with SK ===")
sk = None
for i, data in enumerate(writes):
    d = decrypt(data, KEY_COMM)
    if d[0]==0xE7 and d[1]==0x01 and verify(d):
        sk = bytes(d[2:18])
    elif sk:
        d2 = decrypt(data, sk)
        print(f"  {i}: {d2.hex()} v={verify(d2)}")

# Now analyze the toggle bursts
print("\n=== Toggle burst 1 (entries 1457-1465) ===")
burst1 = [
    bytes.fromhex("2b5d09d62ae6c0aa6f14cd22c826c37fc1ba64da"),
    bytes.fromhex("b2f1c5d47ea1e0e05332b78521c599a4c1ba6480"),
    bytes.fromhex("2a711fb12cb687393772036eaf924729c1ba6482"),
    bytes.fromhex("c7b0c936c3c7fb292700db90497a3767c1ba64a7"),
    bytes.fromhex("4d9d040014c3eb36e311abc02c5c812ac1ba64a6"),
    bytes.fromhex("207168559106fcfa234e5833ffc1d13ec1ba6492"),
    bytes.fromhex("b295a4d9ce1d7ca208e8ac044ecc752cc1ba6435"),
    bytes.fromhex("157695161ceb7579dbd0ac18f8b48a6bc1ba6483"),
    bytes.fromhex("0024e7aea448899d9ba2202fd67030abc1ba6482"),
]
# First 2 with KEY_COMM (handshake), rest with SK
for i, data in enumerate(burst1):
    if i < 2:
        d = decrypt(data, KEY_COMM)
        print(f"  {i}: KEY_COMM: {d.hex()} v={verify(d)}")
        if d[0]==0xE7 and d[1]==0x01 and verify(d):
            sk_burst = bytes(d[2:18])
            print(f"    SK={sk_burst.hex()}")
    else:
        d = decrypt(data, KEY_COMM)
        if d[0]==0xE7 and d[1]==0x02 and verify(d):
            print(f"  {i}: KEY_COMM: {d.hex()} v={verify(d)} (E7 02)")
        elif sk_burst:
            d2 = decrypt(data, sk_burst)
            print(f"  {i}: SK: {d2.hex()} v={verify(d2)}")
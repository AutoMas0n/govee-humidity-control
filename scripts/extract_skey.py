import struct
from Cryptodome.Cipher import AES

KEY_COMM = bytes.fromhex("4d616b696e674c696665536d61727465")

def rc4(d,k):
    S=list(range(256));j=0
    for i in range(256):j=(j+S[i]+k[i%len(k)])&255;S[i],S[j]=S[j],S[i]
    i=j=0;o=bytearray(len(d))
    for n in range(len(d)):i=(i+1)&255;j=(j+S[i])&255;S[i],S[j]=S[j],S[i];o[n]=d[n]^S[(S[i]+S[j])&255]
    return bytes(o)
def dec(p,k):
    return AES.new(k,AES.MODE_ECB).decrypt(p[:16])+rc4(p[16:20],k)
def v(f):
    c=0
    for b in f[:19]:c^=b
    return c==f[19]

with open("/tmp/btsnoop_hci.log","rb") as f:data=f.read()
pos=16;pkts=[]
while pos<len(data):
    if pos+24>len(data):break
    ol,il,fl,dr,tshi,tslo=struct.unpack(">IIIIII",data[pos:pos+24])
    ts=(tshi<<32)|tslo;pos+=24
    if pos+il>len(data):break
    pkt=data[pos:pos+il];pos+=il
    if len(pkt)<1 or pkt[0]!=2:continue
    if len(pkt)>=10:
        cid=struct.unpack("<H",pkt[7:9])[0]
        if cid!=4:continue
        att=pkt[9:];op=att[0];ah=struct.unpack("<H",att[1:3])[0]if len(att)>=3 else 0;pl=att[3:]
        if op==0x52:pkts.append((ts,ah,pl))

# Get notifies
n_pkts = []
for i,(s,ts,ah,p) in enumerate(pkts if False else []):
    pass  # use pkts for all

# Build sessions
sessions = []
for i,(ts,ah,pl) in enumerate(pkts):
    if ah != 0x000E and ah != 0x0011: continue
    if ah == 0x000E and len(pl) >= 20:
        d = dec(pl[:20], KEY_COMM)
        if d[0]==0xE7 and d[1]==0x01 and v(d):
            sessions.append((ts, bytes(d[2:18])))

# Only use handle 0x0011 writes and find 33 B2 data
for sesh_idx, (tssk, sk) in enumerate(sessions):
    next_ts = None
    for t,ah,pl in pkts:
        if ah == 0x000E and len(pl) >= 20:
            d = dec(pl[:20], KEY_COMM)
            if d[0]==0xE7 and d[1]==0x01 and v(d) and t > tssk:
                next_ts = t
                break
    for t,ah,pl in pkts:
        if ah == 0x0011 and t > tssk and (next_ts is None or t < next_ts):
            d = dec(pl, sk)
            if v(d) and d[0]==0x33 and d[1]==0xB2:
                key_bytes = bytes(d[2:19])  # up to 17 bytes of key
                nz = [b for b in key_bytes if b != 0]
                print(f"Session {sesh_idx}: 33 B2 key data: {bytes(nz).hex()} ({len(nz)} bytes)")
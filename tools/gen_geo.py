"""Builds assets/geo/country.bin and assets/geo/flags.bin.

Sources: DB-IP "IP to Country Lite" (CC BY 4.0, https://db-ip.com) and flagcdn.com flag images.
Usage: python tools/gen_geo.py <dbip-country-lite.csv> <flags-dir>
  flags-dir holds <cc>.png files (lowercase two-letter codes).

country.bin: u32 LE n4, n4 * (u32 BE start, 2 bytes code), u32 LE n6, n6 * (u32 BE start of the top
  32 bits, 2 bytes code). A range lasts until the next start; code "ZZ" means unknown.
flags.bin: u16 LE n, n * (2 bytes code, u32 LE offset, u32 LE len), then the PNG data.
"""
import csv, ipaddress, os, struct, sys

csv_path, flags_dir = sys.argv[1:3]
out = os.path.join(os.path.dirname(__file__), "..", "assets", "geo")
os.makedirs(out, exist_ok=True)

v4, v6 = [], []
with open(csv_path, newline="") as f:
    for a, _b, cc in csv.reader(f):
        ip = ipaddress.ip_address(a)
        cc = cc.upper()
        if len(cc) != 2:
            continue
        if ip.version == 4:
            v4.append((int(ip), cc))
        else:
            v6.append((int(ip) >> 96, cc))

def merge(rows):
    rows.sort()
    res = []
    for start, cc in rows:
        if res and res[-1][1] == cc:
            continue
        if res and res[-1][0] == start:
            res[-1] = (start, cc)
            continue
        res.append((start, cc))
    return res

v4, v6 = merge(v4), merge(v6)
with open(os.path.join(out, "country.bin"), "wb") as f:
    f.write(struct.pack("<I", len(v4)))
    for s, cc in v4:
        f.write(struct.pack(">I", s) + cc.encode())
    f.write(struct.pack("<I", len(v6)))
    for s, cc in v6:
        f.write(struct.pack(">I", s) + cc.encode())

files = sorted(n for n in os.listdir(flags_dir) if n.endswith(".png") and len(n) == 6)
head, data = b"", b""
base = 2 + len(files) * 10
for n in files:
    png = open(os.path.join(flags_dir, n), "rb").read()
    head += n[:2].upper().encode() + struct.pack("<II", base + len(data), len(png))
    data += png
with open(os.path.join(out, "flags.bin"), "wb") as f:
    f.write(struct.pack("<H", len(files)) + head + data)
print(len(v4), len(v6), len(files))

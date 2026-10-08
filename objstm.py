from pathlib import Path
import re,zlib
b=Path('E:/10_Learning/Rust/PDFFiller/reference/template.pdf').read_bytes()
for m in re.finditer(rb'(\d+) (\d+) obj\s*<<(.*?)>>\s*stream\s*\r?\n',b,re.S):
 d=m.group(3)
 if b'/Type/ObjStm' not in d and b'/Type /ObjStm' not in d: continue
 start=m.end(); end=b.find(b'endstream',start); raw=b[start:end]
 if raw.startswith(b'\r\n'): raw=raw[2:]
 if b'/Filter/FlateDecode' in d or b'/Filter /FlateDecode' in d: raw=zlib.decompress(raw)
 first=int(re.search(rb'/First\s+(\d+)',d).group(1)); n=int(re.search(rb'/N\s+(\d+)',d).group(1)); header=raw[:first]; body=raw[first:]
 pairs=[(int(a),int(b)) for a,b in re.findall(rb'(\d+)\s+(\d+)',header)]
 print('ObjStm',m.group(1).decode(),'N',n,'First',first)
 for i,(oid,off) in enumerate(pairs):
  nxt=pairs[i+1][1] if i+1<len(pairs) else len(body)
  obj=body[off:nxt]
  if b'/T' in obj or b'/FT' in obj or b'/Subtype' in obj:
   print('OBJ',oid,repr(obj[:900]))

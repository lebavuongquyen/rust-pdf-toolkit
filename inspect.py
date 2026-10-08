from pathlib import Path
import re, hashlib
base=Path('E:/10_Learning/Rust/PDFFiller')
for fn in ['reference/filled.pdf','output/fixed-image.pdf']:
 b=(base/fn).read_bytes(); print('==',fn,len(b),hashlib.sha256(b).hexdigest())
 for term in [b'/T',b'avatar',b'/AcroForm',b'/AP',b'/MK',b'/I',b'/Subtype/Image',b'/Subtype/Form',b'/XObject',b'/BBox',b'/Matrix',b'/Resources',b'/Prev',b'startxref']:
  poss=[m.start() for m in re.finditer(re.escape(term),b)]
  print(term.decode('latin1'),poss[:30],'count',len(poss))
 for p in [m.start() for m in re.finditer(b'/T\(avatar\)',b)]:
  print('AVATAR',repr(b[max(0,p-250):p+1000]))
 for oid in ([39,40] if fn.startswith('reference') else [20,45]):
  m=re.search(rb'\\n'+str(oid).encode()+rb' 0 obj\\n',b)
  print('OBJ',oid,repr(b[m.start():m.start()+1800] if m else b'NOTFOUND'))
 print('TAIL',repr(b[-500:]))

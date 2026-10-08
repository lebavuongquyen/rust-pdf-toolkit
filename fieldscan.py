from pathlib import Path
import re
for fn in ['template.pdf','filled.pdf']:
 b=(Path('E:/10_Learning/Rust/PDFFiller/reference')/fn).read_bytes()
 print('==',fn)
 for m in re.finditer(rb'(\d+) (\d+) obj\r?\n(.*?)endobj',b,re.S):
  body=m.group(3)
  if b'/FT/Btn' in body or b'/FT /Btn' in body or b'/Subtype/Widget' in body or b'/Subtype /Widget' in body:
   print(m.group(1).decode(), body[:900].decode('latin1','replace').replace('\r',' ').replace('\n',' '))

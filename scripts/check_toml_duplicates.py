#!/usr/bin/env python3
from pathlib import Path
import re, collections, sys
p = Path('crates/goldclaw-memory-mini/Cargo.toml')
if not p.exists():
    print('MISSING:', p)
    sys.exit(1)
s = p.read_text(encoding='utf-8', errors='surrogateescape')
lines = s.splitlines()
print(f'path: {p}')
print(f'size: {len(s)} bytes')
print('\nLines with repr:')
for i, line in enumerate(lines, 1):
    print(f'{i:4d}: {line!r}')

headers = re.findall(r'^\s*(\[[^\]]+\])', s, flags=re.M)
print('\nHeader counts:')
for k, v in collections.Counter(headers).items():
    print(f'{k}: {v}')

dups = [k for k,v in collections.Counter(headers).items() if v>1]
if dups:
    print('\nDuplicate headers detected:', dups)
    sys.exit(2)
else:
    print('\nNo duplicate headers detected')
    sys.exit(0)

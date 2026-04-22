#!/usr/bin/env python3
import re
import os
from pathlib import Path

ROOT = Path(os.environ.get('HERMA_ROOT', str(Path.home() / 'herma')))
if not ROOT.exists():
    print('Error: ROOT not found:', ROOT)
    raise SystemExit(1)

PAT = re.compile(r'(path\s*=\s*)(?P<q>["\'])(?P<p>(?:file://)?/home/user/[^"\']+)(?P=q)')

changed = 0
for toml in ROOT.rglob('Cargo.toml'):
    s = toml.read_text()
    def repl(m):
        orig = m.group('p')
        # strip file:// if present
        if orig.startswith('file://'):
            abs_path = orig[len('file://'):]
        else:
            abs_path = orig
        target = Path(abs_path)
        # compute relative path from the Cargo.toml directory
        rel = os.path.relpath(target, start=toml.parent)
        rel = rel.replace(os.path.sep, '/')
        return f"{m.group(1)}{m.group('q')}{rel}{m.group('q')}"

    new_s, n = PAT.subn(repl, s)
    if n > 0 and new_s != s:
        bak = toml.with_suffix(toml.suffix + '.bak')
        bak.write_text(s)
        toml.write_text(new_s)
        changed += 1
        print(f'Patched {toml} ({n} replacements) -> backup: {bak}')

print(f'Done. Files changed: {changed}')

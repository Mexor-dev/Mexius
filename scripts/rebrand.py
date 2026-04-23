#!/usr/bin/env python3
import os
import sys
from pathlib import Path

root = Path(__file__).resolve().parent.parent
targets = [root / 'web' / 'src', root / 'web' / 'public', root / 'web']

TEXT_EXTS = {'.ts', '.tsx', '.js', '.jsx', '.json', '.html', '.css', '.md', '.svg', '.txt', '.env', '.toml'}

def replace_in_file(p: Path):
    if p.suffix.lower() not in TEXT_EXTS and p.suffix != '':
        return
    try:
        s = p.read_text(encoding='utf-8')
    except Exception:
        return
    orig = s
    s = s.replace('ZeroClaw', 'Herma')
    s = s.replace('Zeroclaw', 'Herma')
    s = s.replace('zero-claw', 'herma')
    s = s.replace('Goldclaw', 'Herma')
    s = s.replace('zeroclaw_token', 'herma_token')
    s = s.replace('zeroclaw_session_id', 'herma_session_id')
    s = s.replace('zeroclaw_chat_history_v1', 'herma_chat_history_v1')
    s = s.replace('zeroclaw-unauthorized', 'herma-unauthorized')
    s = s.replace('zeroclaw.v1', 'herma.v1')
    s = s.replace('__ZEROCLAW_GATEWAY__', '__HERMA_GATEWAY__')
    s = s.replace('__ZEROCLAW_BASE__', '__HERMA_BASE__')
    s = s.replace('ZEROCLAW_GATEWAY_PORT', 'HERMA_GATEWAY_PORT')
    s = s.replace('/_app/zeroclaw-trans.png', '/herma.png')
    if s != orig:
        p.write_text(s, encoding='utf-8')
        print('Updated', p)

def rename_asset(p: Path):
    name = p.name
    newname = name
    newname = newname.replace('zeroclaw', 'herma').replace('ZeroClaw', 'Herma').replace('zero-claw', 'herma').replace('Goldclaw', 'Herma')
    if newname != name:
        try:
            p.rename(p.with_name(newname))
            print('Renamed', p, '->', p.with_name(newname))
        except Exception as e:
            print('Failed rename', p, e)

def walk_and_apply(base: Path):
    if not base.exists():
        return
    for p in base.rglob('*'):
        if p.is_file():
            replace_in_file(p)
            if any(x in p.name.lower() for x in ['zeroclaw', 'zero-claw', 'goldclaw']):
                if p.suffix.lower() in ['.png', '.svg', '.jpg', '.jpeg', '.webp']:
                    rename_asset(p)

def main():
    for t in targets:
        walk_and_apply(t)
    print('Rebrand complete')

if __name__ == '__main__':
    main()

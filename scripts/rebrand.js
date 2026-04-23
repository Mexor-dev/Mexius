#!/usr/bin/env node
const fs = require('fs');
const path = require('path');

const root = path.resolve(__dirname, '..');
const targets = [
  path.join(root, 'web', 'src'),
  path.join(root, 'web', 'public'),
  path.join(root, 'web'),
];

const textFileExt = new Set(['.ts', '.tsx', '.js', '.jsx', '.json', '.html', '.css', '.md', '.svg', '.txt', '.env', '.toml']);

function walk(dir, cb) {
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const e of entries) {
    const p = path.join(dir, e.name);
    if (e.isDirectory()) {
      walk(p, cb);
    } else {
      cb(p);
    }
  }
}

function replaceInFile(file) {
  const ext = path.extname(file).toLowerCase();
  if (!textFileExt.has(ext) && ext !== '') return;
  let s;
  try {
    s = fs.readFileSync(file, 'utf8');
  } catch (e) {
    return;
  }
  let orig = s;
  // Replace various case-insensitive forms but keep capitalization simple
  s = s.replace(/ZeroClaw/g, 'Herma');
  s = s.replace(/Zeroclaw/g, 'Herma');
  s = s.replace(/zero-claw/g, 'herma');
  s = s.replace(/Goldclaw/g, 'Herma');
  s = s.replace(/zeroclaw_token/g, 'herma_token');
  s = s.replace(/zeroclaw_session_id/g, 'herma_session_id');
  s = s.replace(/zeroclaw_chat_history_v1/g, 'herma_chat_history_v1');
  s = s.replace(/zeroclaw/g, 'herma');
  s = s.replace(/ZEROCLAW/g, 'HERMA');
  s = s.replace(/__ZEROCLAW_GATEWAY__/g, '__HERMA_GATEWAY__');
  s = s.replace(/__ZEROCLAW_BASE__/g, '__HERMA_BASE__');
  s = s.replace(/ZEROCLAW_GATEWAY_PORT/g, 'HERMA_GATEWAY_PORT');
  s = s.replace(/zeroclaw-unauthorized/g, 'herma-unauthorized');
  s = s.replace(/zeroclaw.v1/g, 'herma.v1');
  s = s.replace(/127\.0\.0\.1:42617/g, '127.0.0.1:42617');
  s = s.replace(/_app\/zeroclaw-trans.png/g, 'herma.png');

  if (s !== orig) {
    fs.writeFileSync(file, s, 'utf8');
    console.log('Updated', file);
  }
}

function renameAsset(file) {
  const dir = path.dirname(file);
  const base = path.basename(file);
  const newid = base
    .replace(/zeroclaw/gi, 'herma')
    .replace(/zero-claw/gi, 'herma')
    .replace(/Goldclaw/gi, 'Herma');
  if (newid !== base) {
    const newpath = path.join(dir, newid);
    try {
      fs.renameSync(file, newpath);
      console.log('Renamed', file, '->', newpath);
    } catch (e) {
      console.warn('Failed renaming', file, e.message);
    }
  }
}

for (const t of targets) {
  if (!fs.existsSync(t)) continue;
  walk(t, (file) => {
    replaceInFile(file);
    const base = path.basename(file);
    if (/zeroclaw|zero-claw|Goldclaw/i.test(base)) {
      // Only rename image assets
      const ext = path.extname(file).toLowerCase();
      if (['.png', '.svg', '.jpg', '.jpeg', '.webp'].includes(ext)) {
        renameAsset(file);
      }
    }
  });
}

console.log('Rebrand pass complete.');

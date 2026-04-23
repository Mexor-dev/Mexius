#!/bin/bash
set -e

git checkout -b feature/spa-route-logging-test >/dev/null 2>&1 || true

git add -A

if git diff --cached --quiet; then
  echo NO_STAGED_CHANGES
else
  git commit -m "feat: gateway debug logging static helper SPA tests"
fi

git rev-parse --abbrev-ref HEAD

git log -n 1 --pretty=oneline || true

git status --porcelain || true

git branch -v || true

git push -u origin feature/spa-route-logging-test || echo PUSH_FAILED

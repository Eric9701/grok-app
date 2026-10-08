#!/usr/bin/env bash
# Build a Windows portable (绿色版) zip from a release Windows binary and upload to GitHub Release.
# Usage (CI): bash scripts/package-windows-portable.sh v0.1.2
# Or: TAG=v0.1.2 bash scripts/package-windows-portable.sh
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT"

TAG="${1:-${TAG:-}}"
if [[ -z "$TAG" ]]; then
  TAG="v$(python3 -c 'import json; print(json.load(open("package.json"))["version"])')"
fi
VER="${TAG#v}"
PRODUCT="$(
  python3 -c 'import json; print(json.load(open("src-tauri/tauri.conf.json", encoding="utf-8"))["productName"])'
)"
if [[ -z "$PRODUCT" ]]; then
  echo "error: productName missing in src-tauri/tauri.conf.json" >&2
  exit 1
fi

# productName is Atlas (or Grok upstream). Cargo package name may still be grok-app.exe.
find_release_exe() {
  local name
  for name in "${PRODUCT}.exe" grok-app.exe Grok.exe; do
    # Prefer top-level release binary (not under bundle/)
    if [[ -f "src-tauri/target/release/${name}" ]]; then
      echo "src-tauri/target/release/${name}"
      return 0
    fi
  done
  # Fallback: any release/*.exe that is not under bundle/
  local found
  found="$(find src-tauri/target -type f -name '*.exe' 2>/dev/null \
    | grep -E '/release/[^/]+\.exe$' \
    | grep -v '/bundle/' \
    | head -n 1 || true)"
  if [[ -n "$found" && -f "$found" ]]; then
    echo "$found"
    return 0
  fi
  return 1
}

EXE="$(find_release_exe || true)"
if [[ -z "${EXE:-}" || ! -f "$EXE" ]]; then
  echo "error: Windows release .exe not found under src-tauri/target/release/" >&2
  find src-tauri/target -name '*.exe' 2>/dev/null | head -40 || true
  exit 1
fi
echo "using EXE=$EXE"

STAGE="dist-portable/${PRODUCT}_${VER}_x64-portable"
rm -rf dist-portable
mkdir -p "$STAGE"
# Ship under the product name (Atlas.exe on this fork).
cp "$EXE" "$STAGE/${PRODUCT}.exe"
python3 - "$VER" "$STAGE" "$PRODUCT" <<'PY'
import sys
from pathlib import Path

ver, stage, product = sys.argv[1], Path(sys.argv[2]), sys.argv[3]
(stage / "README-portable.txt").write_text(
    f"""{product} portable (绿色版) v{ver}
================================
1. 解压本目录到任意位置（无需安装）。
2. 双击 {product}.exe 运行。
3. 需要系统已安装 Microsoft Edge WebView2 Runtime（Win10/11 通常已自带）。
4. 真 Agent 能力仍需本机 {product} CLI，并完成登录。
5. SmartScreen 可能提示未知发布者 → 更多信息 → 仍要运行。

Extract anywhere and run {product}.exe. WebView2 required. {product} CLI still needed for agent sessions.
""",
    encoding="utf-8",
)
print("wrote", stage / "README-portable.txt")
PY

OUT="${PRODUCT}_${VER}_x64-portable.zip"
# Prefer zip; on Windows Git Bash it is usually present. Fallback to PowerShell Compress-Archive.
if command -v zip >/dev/null 2>&1; then
  (cd dist-portable && zip -r "../${OUT}" "${PRODUCT}_${VER}_x64-portable")
else
  powershell.exe -NoProfile -Command \
    "Compress-Archive -Path 'dist-portable/${PRODUCT}_${VER}_x64-portable' -DestinationPath '${OUT}' -Force"
fi
ls -lah "$OUT"
if command -v gh >/dev/null 2>&1 && [[ -n "${GH_TOKEN:-${GITHUB_TOKEN:-}}" ]]; then
  gh release upload "$TAG" "$OUT" --clobber
  echo "uploaded $OUT to $TAG"
else
  echo "skip upload (gh/token missing); artifact at $OUT"
fi

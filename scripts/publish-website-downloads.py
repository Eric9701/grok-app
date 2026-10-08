#!/usr/bin/env python3
"""Stable installer aliases + downloads.json for grok-app.com.

The future official site must not host DMG/EXE (traffic cost). Buttons point at
GitHub Releases. Versioned asset names include the semver, so this script also
publishes unchanging aliases:

  https://github.com/<repo>/releases/latest/download/<productName>_mac_x64.dmg
  https://github.com/<repo>/releases/latest/download/<productName>_windows_x64-setup.exe

The filename prefix is tauri.conf.json productName (Atlas on this fork).

Usage (CI, after versioned assets are on the tag):
  python3 scripts/publish-website-downloads.py \\
    --dir /tmp/release-assets --tag v0.2.20 --repo RongleCat/grok-app --write-aliases

  python3 scripts/publish-website-downloads.py --self-test
"""
from __future__ import annotations

import argparse
import hashlib
import json
import shutil
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any

SCHEMA_VERSION = 1
OFFICIAL_SITE = "https://grok-app.com"
DOWNLOADS_JSON_NAME = "downloads.json"

# Every official installer must exist or the job fails (no half-built Latest).
REQUIRED_IDS = (
    "mac-aarch64",
    "mac-x64",
    "windows-x64",
    "windows-x64-portable",
    "linux-x64-appimage",
    "linux-x64-deb",
    "linux-x64-rpm",
)

def product_name() -> str:
    """Tauri bundle prefix. This fork is Atlas; upstream Grok stays Grok."""
    conf = Path(__file__).resolve().parents[1] / "src-tauri" / "tauri.conf.json"
    name = json.loads(conf.read_text(encoding="utf-8")).get("productName")
    if not isinstance(name, str) or not name.strip():
        raise SystemExit(f"error: productName missing in {conf}")
    return name.strip()


def installer_spec(product: str) -> tuple[dict[str, Any], ...]:
    """source_names: first existing file wins. `{ver}` is the tag without leading v."""
    p = product
    return (
        {
            "id": "mac-aarch64",
            "os": "macos",
            "arch": "aarch64",
            "kind": "dmg",
            "label": "macOS Apple Silicon",
            "stable": f"{p}_mac_aarch64.dmg",
            "sources": (f"{p}_{{ver}}_aarch64.dmg",),
        },
        {
            "id": "mac-x64",
            "os": "macos",
            "arch": "x64",
            "kind": "dmg",
            "label": "macOS Intel",
            "stable": f"{p}_mac_x64.dmg",
            "sources": (f"{p}_{{ver}}_x64.dmg",),
        },
        {
            "id": "windows-x64",
            "os": "windows",
            "arch": "x64",
            "kind": "nsis",
            "label": "Windows x64",
            "stable": f"{p}_windows_x64-setup.exe",
            "sources": (f"{p}_{{ver}}_x64-setup.exe",),
        },
        {
            "id": "windows-x64-portable",
            "os": "windows",
            "arch": "x64",
            "kind": "portable-zip",
            "label": "Windows x64 portable",
            "stable": f"{p}_windows_x64-portable.zip",
            "sources": (f"{p}_{{ver}}_x64-portable.zip",),
        },
        {
            "id": "linux-x64-appimage",
            "os": "linux",
            "arch": "x64",
            "kind": "appimage",
            "label": "Linux x64 AppImage",
            "stable": f"{p}_linux_x64.AppImage",
            "sources": (f"{p}_{{ver}}_amd64.AppImage", f"{p}_{{ver}}_x86_64.AppImage"),
        },
        {
            "id": "linux-x64-deb",
            "os": "linux",
            "arch": "x64",
            "kind": "deb",
            "label": "Linux x64 .deb",
            "stable": f"{p}_linux_x64.deb",
            "sources": (f"{p}_{{ver}}_amd64.deb", f"{p}_{{ver}}_x86_64.deb"),
        },
        {
            "id": "linux-x64-rpm",
            "os": "linux",
            "arch": "x64",
            "kind": "rpm",
            "label": "Linux x64 .rpm",
            "stable": f"{p}_linux_x64.rpm",
            "sources": (
                f"{p}-{{ver}}-1.x86_64.rpm",
                f"{p}_{{ver}}-1.x86_64.rpm",
                f"{p}-{{ver}}.x86_64.rpm",
                f"{p}_{{ver}}_x86_64.rpm",
                f"{p}_{{ver}}_amd64.rpm",
            ),
        },
    )


def normalize_tag(raw: str) -> tuple[str, str]:
    tag = raw.strip()
    if not tag:
        raise ValueError("empty tag")
    ver = tag[1:] if tag[:1] in {"v", "V"} else tag
    if not ver:
        raise ValueError(f"invalid tag: {raw!r}")
    return (tag if tag[:1] in {"v", "V"} else f"v{ver}", ver)


def sha256_file(path: Path) -> str:
    h = hashlib.sha256()
    with path.open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def find_source(directory: Path, ver: str, sources: tuple[str, ...]) -> Path | None:
    for pattern in sources:
        candidate = directory / pattern.format(ver=ver)
        if candidate.is_file() and not candidate.name.endswith(".sig"):
            return candidate
    return None


def github_latest_url(repo: str, filename: str) -> str:
    return f"https://github.com/{repo}/releases/latest/download/{filename}"


def github_versioned_url(repo: str, tag: str, filename: str) -> str:
    return f"https://github.com/{repo}/releases/download/{tag}/{filename}"


def build_manifest(
    directory: Path,
    *,
    tag: str,
    repo: str,
    write_aliases: bool,
    product: str | None = None,
) -> dict[str, Any]:
    tag, ver = normalize_tag(tag)
    product = product or product_name()
    installers: dict[str, Any] = {}
    upload_names: list[str] = [DOWNLOADS_JSON_NAME]

    for spec in installer_spec(product):
        source = find_source(directory, ver, spec["sources"])
        if source is None:
            continue
        stable_name: str = spec["stable"]
        dest = directory / stable_name
        if write_aliases:
            if dest.resolve() != source.resolve():
                shutil.copy2(source, dest)
        elif not dest.is_file():
            dest = source
        digest = sha256_file(dest if dest.is_file() else source)
        size = (dest if dest.is_file() else source).stat().st_size
        if dest.is_file() and dest.name == stable_name:
            upload_names.append(stable_name)
        installers[spec["id"]] = {
            "id": spec["id"],
            "os": spec["os"],
            "arch": spec["arch"],
            "kind": spec["kind"],
            "label": spec["label"],
            "filename": stable_name,
            "url": github_latest_url(repo, stable_name),
            "versionedFilename": source.name,
            "versionedUrl": github_versioned_url(repo, tag, source.name),
            "sha256": digest,
            "size": size,
        }

    missing = [i for i in REQUIRED_IDS if i not in installers]
    if missing:
        have = ", ".join(sorted(p.name for p in directory.iterdir() if p.is_file())) or "(none)"
        raise SystemExit(
            f"error: website download contract missing required installers: "
            f"{', '.join(missing)}\nfiles in {directory}: {have}"
        )

    manifest = {
        "schemaVersion": SCHEMA_VERSION,
        "product": product,
        "officialSite": OFFICIAL_SITE,
        "version": ver,
        "tag": tag,
        "releaseUrl": f"https://github.com/{repo}/releases/tag/{tag}",
        "downloadsJsonUrl": github_latest_url(repo, DOWNLOADS_JSON_NAME),
        "installers": installers,
    }
    return {"manifest": manifest, "uploadNames": upload_names}


def write_outputs(
    directory: Path,
    payload: dict[str, Any],
    *,
    json_out: Path,
    upload_list: Path,
) -> None:
    manifest = payload["manifest"]
    json_out.write_text(
        json.dumps(manifest, indent=2, ensure_ascii=False) + "\n",
        encoding="utf-8",
    )
    names: list[str] = []
    seen: set[str] = set()
    for name in payload["uploadNames"]:
        if name in seen:
            continue
        seen.add(name)
        path = directory / name if name != json_out.name else json_out
        if name == json_out.name or path.is_file():
            names.append(name)
    upload_list.write_text("\n".join(names) + "\n", encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dir", type=Path, help="Directory of downloaded release assets")
    parser.add_argument("--tag", help="Release tag, e.g. v0.2.20")
    parser.add_argument("--repo", default="RongleCat/grok-app", help="owner/name")
    parser.add_argument(
        "--write-aliases",
        action="store_true",
        help="Copy versioned installers to stable filenames in --dir",
    )
    parser.add_argument("--json-out", type=Path, help="Override downloads.json path")
    parser.add_argument(
        "--upload-list",
        type=Path,
        help="Write alias + downloads.json filenames (default: --dir/.website-upload.txt)",
    )
    parser.add_argument("--self-test", action="store_true")
    args = parser.parse_args(argv)

    if args.self_test:
        return _run_self_test()

    if args.dir is None or args.tag is None:
        parser.error("--dir and --tag are required (or pass --self-test)")

    directory = args.dir.resolve()
    if not directory.is_dir():
        print(f"error: not a directory: {directory}", file=sys.stderr)
        return 1

    payload = build_manifest(
        directory,
        tag=args.tag,
        repo=args.repo,
        write_aliases=args.write_aliases,
    )
    json_out = (args.json_out or (directory / DOWNLOADS_JSON_NAME)).resolve()
    upload_list = (args.upload_list or (directory / ".website-upload.txt")).resolve()
    write_outputs(directory, payload, json_out=json_out, upload_list=upload_list)
    print(f"wrote {json_out}")
    for installer_id, row in payload["manifest"]["installers"].items():
        print(f"  {installer_id}: {row['filename']} ({row['size']} bytes)")
    return 0


class WebsiteDownloadsTests(unittest.TestCase):
    def test_normalize_tag(self) -> None:
        self.assertEqual(normalize_tag("v0.2.20"), ("v0.2.20", "0.2.20"))
        self.assertEqual(normalize_tag("0.2.20"), ("v0.2.20", "0.2.20"))

    def test_builds_stable_urls_and_aliases(self) -> None:
        product = product_name()
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            samples = {
                f"{product}_0.2.20_aarch64.dmg": b"arm-dmg",
                f"{product}_0.2.20_x64.dmg": b"intel-dmg",
                f"{product}_0.2.20_x64-setup.exe": b"win-setup",
                f"{product}_0.2.20_x64-portable.zip": b"win-zip",
                f"{product}_0.2.20_amd64.AppImage": b"appimage",
                f"{product}_0.2.20_amd64.deb": b"deb",
                f"{product}_0.2.20-1.x86_64.rpm": b"rpm",
                f"{product}_0.2.20_x64.dmg.sig": b"ignore-me",
            }
            for name, body in samples.items():
                (root / name).write_bytes(body)

            payload = build_manifest(
                root,
                tag="v0.2.20",
                repo="RongleCat/grok-app",
                write_aliases=True,
            )
            installers = payload["manifest"]["installers"]
            self.assertEqual(
                installers["mac-x64"]["url"],
                f"https://github.com/RongleCat/grok-app/releases/latest/download/{product}_mac_x64.dmg",
            )
            self.assertEqual(
                installers["windows-x64"]["url"],
                f"https://github.com/RongleCat/grok-app/releases/latest/download/{product}_windows_x64-setup.exe",
            )
            self.assertEqual(
                installers["mac-x64"]["versionedUrl"],
                f"https://github.com/RongleCat/grok-app/releases/download/v0.2.20/{product}_0.2.20_x64.dmg",
            )
            self.assertEqual(installers["mac-x64"]["sha256"], hashlib.sha256(b"intel-dmg").hexdigest())
            self.assertEqual((root / f"{product}_mac_x64.dmg").read_bytes(), b"intel-dmg")
            self.assertEqual((root / f"{product}_windows_x64-setup.exe").read_bytes(), b"win-setup")
            self.assertIn("linux-x64-rpm", installers)
            self.assertEqual(
                installers["linux-x64-rpm"]["versionedFilename"],
                f"{product}_0.2.20-1.x86_64.rpm",
            )
            self.assertEqual(payload["manifest"]["product"], product)
            self.assertNotIn("sig", json.dumps(installers))
            self.assertEqual(payload["manifest"]["officialSite"], OFFICIAL_SITE)
            self.assertEqual(payload["manifest"]["schemaVersion"], SCHEMA_VERSION)

            json_out = root / DOWNLOADS_JSON_NAME
            upload_list = root / ".website-upload.txt"
            write_outputs(root, payload, json_out=json_out, upload_list=upload_list)
            parsed = json.loads(json_out.read_text(encoding="utf-8"))
            self.assertEqual(parsed["installers"]["mac-aarch64"]["filename"], f"{product}_mac_aarch64.dmg")
            listed = upload_list.read_text(encoding="utf-8").splitlines()
            self.assertIn(DOWNLOADS_JSON_NAME, listed)
            self.assertIn(f"{product}_mac_x64.dmg", listed)
            self.assertIn(f"{product}_windows_x64-setup.exe", listed)

    def test_missing_required_fails(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / f"{product_name()}_0.2.20_aarch64.dmg").write_bytes(b"arm")
            with self.assertRaises(SystemExit) as ctx:
                build_manifest(
                    root,
                    tag="v0.2.20",
                    repo="RongleCat/grok-app",
                    write_aliases=False,
                )
            self.assertIn("mac-x64", str(ctx.exception))
            self.assertIn("windows-x64", str(ctx.exception))


def _run_self_test() -> int:
    suite = unittest.defaultTestLoader.loadTestsFromTestCase(WebsiteDownloadsTests)
    result = unittest.TextTestRunner(verbosity=2).run(suite)
    return 0 if result.wasSuccessful() else 1


if __name__ == "__main__":
    raise SystemExit(main())

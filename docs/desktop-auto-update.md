# Desktop auto-update

Grok App uses the same **Tauri 2 updater** shape as Minos / Buzz: signed release
artifacts, a rolling `latest.json` endpoint, in-app check/download/install, and
a hard stop of managed agent / mirror / voice / IM processes before binary swap.

## Architecture

```
CI release
  ├── vX.Y.Z                 user-facing installers (DMG / AppImage / NSIS …)
  └── grok-desktop-latest    rolling updater release
        └── latest.json  + per-platform archive + .sig
                 ▲
                 │ check()
        Desktop  tauri-plugin-updater  (release builds only)
                 │ prepare_for_app_update → stop agents / mirror / voice / IM
                 │ install + relaunch
        UI: Settings → About
```

Unsigned / local builds keep the previous GitHub “open release page” path.

## Atlas 白标：无签名自更新

`feat-atlas` 不使用 tauri-plugin-updater（它要求 minisign 公钥），改用 Host 自带的更新器 `src-tauri/src/self_update/`，前端仍复用 `useUpdater` 的状态机。

```
atlas-server  /atlas/desktop/{channel}/update.json   （按请求来源生成 URL，含 sha256 + size）
              /atlas/desktop/{channel}/<安装包>      （http.ServeFile，支持 Range）
                 ▲
                 │ self_update_check → self_update_download（.part 断点续传 → 校验 → 改名）
        Host  updates/<version>/   pending.json（启动时判定上次安装成功或失败）
                 │ 用户在「更新已就绪」卡片点击「更新」
                 │ self_update_install：重新校验哈希 → 写 pending →
                 │   Windows 安装版：prepare_for_app_update → NSIS /P /R /UPDATE → 退出
                 │   Windows 绿色版：解压 *-portable.zip → 辅助脚本等进程退出后覆盖目录并重新打开
                 │   macOS：tar 解包 → 去 quarantine → 替换 .app（失败回滚）→ 前端 prepare + relaunch
```

- 信任模型：只校验服务器 feed 里的 SHA-256 和大小，payload URL 必须与 feed 同源、同目录。没有签名，所以生产环境应使用 HTTPS 镜像
- Windows 安装版发布 `*-setup.exe`（NSIS，currentUser），平台键 `windows-x86_64`。绿色版另发布 `*-portable.zip`，平台键 `windows-x86_64-portable`，目录里要保留 `README-portable.txt`。macOS 发布 `.app.tar.gz`（未签名的 `.app` 首次安装仍需用户手动放行，之后的更新会去掉 quarantine 属性）
- 失败计数：同一版本安装失败 2 次后不再自动弹窗，关于页仍可手动重试，或下载安装包

## App pieces

| Piece | Location |
|-------|----------|
| Build-time gate | `build.rs` → `cfg(grok_updater_enabled)` when both `GROK_UPDATER_*` env vars are set (crate always linked for ACL) |
| Release conf delta | `scripts/build-release-config.mjs` → `src-tauri/tauri.release.conf.json` (gitignored — always regenerate) |
| Plugin register | `src-tauri/src/lib.rs` (cfg + non-debug only) |
| Platform support | `is_auto_update_supported` — Linux requires AppImage (`APPIMAGE` env) |
| Pre-relaunch teardown | `prepare_for_app_update` — **only after** successful `install()`, never before |
| Frontend state machine | `src/hooks/useUpdater.ts` + `UpdaterProvider` (single path: plugin or GitHub) |
| Path honesty (copy / channel) | `src/lib/appUpdateHonesty.ts` — signed auto vs GitHub manual vs unsupported vs host-only; soft-fail error classes; agents stop only after install prepare |
| UI | Settings → About (`AboutUpdateRow`) |
| Capabilities | `updater:allow-*`, `process:allow-restart` |

Local `pnpm dev` / debug builds **never** enable the updater plugin (no
feature, no env), so dev binaries never hit a production endpoint.

### Install / teardown order (P0)

```
download → install() → prepare_for_app_update() → relaunch()
```

If `install()` fails, agents / voice / IM / mirror stay running.

## Secrets (GitHub Actions)

| Secret / variable | Purpose |
|-------------------|---------|
| `GROK_UPDATER_PUBLIC_KEY` | minisign public key embedded in the app |
| `TAURI_SIGNING_PRIVATE_KEY` | minisign private key for signing updater archives |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | password for the private key (empty string OK) |
| Apple signing / notarize secrets | codesign + notarize DMG / .app (recommended for macOS Gatekeeper) |

Generate a keypair once (see [Tauri updater](https://v2.tauri.app/plugin/updater/)):

```sh
pnpm tauri signer generate -w ~/.tauri/grok-app.key
# public key → GROK_UPDATER_PUBLIC_KEY
# private key file contents → TAURI_SIGNING_PRIVATE_KEY
```

### Maintainer production checklist

Before treating silent update as “on” for users:

1. **Secrets in the shipping repo** (Settings → Secrets and variables → Actions): all four rows above that you use. Empty `TAURI_SIGNING_PRIVATE_KEY` must not be set — omit or set a real key.
2. **Local dry-run (no secret values printed):**
   ```sh
   ./scripts/verify-updater-setup.sh
   ./scripts/verify-updater-setup.sh --fetch-latest
   ```
3. **Release cut:** tag `vX.Y.Z` so CI builds installers **and** refreshes `grok-desktop-latest` + `latest.json` + `.sig`.
4. **Smoke on a prior signed build:** Settings → About shows **Update channel: in-app (signed release)** → Check → Download → Install and restart → version matches tag.
5. **Failure path:** if install fails, agents / Remote IM / mirror must keep running (`prepare_for_app_update` only after successful `install()`).
6. **Unsigned / local builds:** About must show **GitHub download** channel and still open Release / download installer (no crash; never claims silent update).
7. **Linux non-AppImage:** About shows **unsupported** package-type channel + AppImage-only note when the plugin is compiled in.

In-app host command `updater_status` reports `{ channel, pluginEnabled, platformSupported, endpoint }` for Doctor / About (`channel` is `silent` | `github_manual` | `unsupported`).

## Rolling endpoint

```text
https://github.com/<owner>/grok-app/releases/download/grok-desktop-latest/latest.json
```

Publish two GitHub releases per cut:

1. **`vX.Y.Z`** — human installers + notes + **stable aliases** (`Grok_mac_x64.dmg`, `Grok_windows_x64-setup.exe`, …) + `downloads.json` for grok-app.com
2. **`grok-desktop-latest`** — updater archives + `latest.json` (clobber each release)

Do **not** point website download buttons at `grok-desktop-latest`. That tag is the silent updater channel. First-time installs use `/releases/latest/download/<stable-alias>` (see `docs/llm-wiki/release.md`).

## Build steps (outline)

```sh
export GROK_UPDATER_PUBLIC_KEY=...
export GROK_UPDATER_ENDPOINT=https://github.com/<owner>/grok-app/releases/download/grok-desktop-latest/latest.json
export TAURI_SIGNING_PRIVATE_KEY=...
export TAURI_SIGNING_PRIVATE_KEY_PASSWORD=...

# 1) Write tauri.release.conf.json (gitignored — required before --config)
node scripts/build-release-config.mjs

# 2) Same GROK_UPDATER_* env must still be set so build.rs enables registration
pnpm tauri build --config src-tauri/tauri.release.conf.json
```

Without step 1, `tauri build --config src-tauri/tauri.release.conf.json` fails with
file not found. The crate is always a hard dependency (Tauri ACL); only
**registration** is gated by the env cfg.

After all platforms upload assets to `vX.Y.Z`:

```sh
TAG=v0.1.9 REPO=<owner>/grok-app bash scripts/assemble-updater-manifest.sh
```

Platform keys: `darwin-aarch64`, `darwin-x86_64`, `linux-x86_64`, `windows-x86_64`.

## Linux note

Only **AppImage** supports in-app update. `.deb` / `.rpm` installs report
channel **`unsupported`** (About: package-type cannot auto-update + AppImage
note) and surface `manual-required` when a newer build is known so the user can
open GitHub Releases.

## macOS note

Codesign + notarize the `.app` / DMG in CI when Apple secrets are present.
After notarization, rebuild the updater `.tar.gz` from the signed app and
re-sign with the Tauri updater key (same pattern as Buzz) if you notarize
post-build.

## Manual verification

1. `pnpm typecheck` / `pnpm test` — UI unit tests
2. `cargo test --manifest-path src-tauri/Cargo.toml updater::` — Rust helpers
3. Settings → About shows **manual GitHub check** on local builds (expected)
4. Release smoke: build with both env vars, confirm `is_updater_plugin_enabled`
   is true in a release binary, and that check hits `latest.json`

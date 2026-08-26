/**
 * Settings → Appearance → Theme: local presets + import/export.
 * Hidden when not the desktop host.
 */

import { useCallback, useEffect, useState, type CSSProperties } from "react";
import { isDesktopHost } from "@/lib/api";
import {
  skinPackExport,
  skinPickOpen,
  skinPickSave,
  skinPresetDelete,
  skinPresetExport,
  skinPresetList,
  skinPresetRename,
  skinPresetReplaceFromUpload,
  skinPresetSaveFromUpload,
  type SkinPresetListItem,
} from "@/lib/api/skin";
import {
  loadActivePresetId,
  notifySkinLibraryChanged,
  resolveActivePresetId,
  saveActivePresetId,
  subscribeSkinLibraryChanged,
} from "@/lib/skinActivePreset";
import { officialCatalogConfigured } from "@/lib/skinCatalog";
import { exportFileName, parseSkinPackError, type SkinPackErrorCode } from "@/lib/skinPack";
import {
  currentLookManifest,
  uploadCurrentWallpaper,
} from "@/lib/skinPresetStore";
import { subscribeAppearanceWriteBusy } from "@/lib/appearanceWriteLock";
import { mediaVideoPoster } from "@/lib/api/fs";
import {
  ensureMediaEndpoint,
  resolveImageSrc,
  resolveImageSrcSync,
} from "@/lib/imageSrc";
import {
  getThemeSkinMeta,
  isThemeSkinId,
  type ThemeSkinId,
} from "@/lib/themeSkin";
import { useThemeShell } from "@/providers/ThemeProvider";
import { useSkinShare } from "@/providers/SkinShareProvider";
import { useSettingsModel } from "@/providers/SettingsModelContext";
import { GlassModal } from "@/components/GlassModal";
import { Tip } from "@/components/ui/tooltip";
import {
  IconCheck,
  IconExportImage,
  IconFolder,
  IconPlus,
  IconRefresh,
  IconRename,
  IconRewind,
  IconSparkles,
  IconTrash,
  IconUpload,
} from "@/components/icons";
import { SkinCatalogModal } from "./SkinCatalogModal";
import { SkinSourcesModal } from "./SkinSourcesModal";

function isVideoAssetPath(path: string): boolean {
  const lower = path.toLowerCase();
  return lower.endsWith(".mp4") || lower.endsWith(".webm");
}

function presetCardStyle(p: SkinPresetListItem, thumbSrc?: string): CSSProperties {
  if (thumbSrc) {
    return {
      backgroundImage: `linear-gradient(180deg, rgb(0 0 0 / 0.15), rgb(0 0 0 / 0.55)), url(${thumbSrc})`,
      backgroundSize: "cover",
      backgroundPosition: "center",
    };
  }
  const skinId = (isThemeSkinId(p.skin) ? p.skin : "default") as ThemeSkinId;
  const meta = getThemeSkinMeta(skinId);
  return {
    backgroundImage: `linear-gradient(135deg, ${meta.swatch} 0%, ${meta.swatchAlt} 100%)`,
  };
}

/** Resolve a CSS-usable thumb URL; videos need a still poster, not the mp4 URL. */
async function resolvePresetThumbSrc(
  p: SkinPresetListItem,
): Promise<string | null> {
  const candidates = [p.thumbPath, p.wallpaperPath].filter(
    (x): x is string => !!x,
  );
  for (const path of candidates) {
    if (isVideoAssetPath(path)) {
      try {
        const poster = await mediaVideoPoster(path);
        if (!poster?.posterPath) continue;
        const src =
          resolveImageSrcSync(poster.posterPath) ||
          (await resolveImageSrc(poster.posterPath));
        if (src) return src;
      } catch {
        /* ffmpeg missing / path denied — keep trying */
      }
      continue;
    }
    const src = resolveImageSrcSync(path) || (await resolveImageSrc(path));
    if (src) return src;
  }
  return null;
}

export function SkinPresetsCard() {
  const desktop = isDesktopHost();
  const theme = useThemeShell();
  const share = useSkinShare();
  const s = useSettingsModel() as { t: (k: string, v?: Record<string, string | number>) => string };
  const t = s.t;
  const [presets, setPresets] = useState<SkinPresetListItem[]>([]);
  const [thumbSrcById, setThumbSrcById] = useState<Record<string, string>>({});
  const [usage, setUsage] = useState({ bytes: 0, budget: 0, hasUndo: false });
  const [busy, setBusy] = useState(false);
  const [writeBusy, setWriteBusy] = useState(false);
  const [nameOpen, setNameOpen] = useState<"save" | "export" | "rename" | null>(null);
  const [nameValue, setNameValue] = useState("");
  const [renameId, setRenameId] = useState<string | null>(null);
  const [deleteId, setDeleteId] = useState<string | null>(null);
  const [updateId, setUpdateId] = useState<string | null>(null);
  const [catalogOpen, setCatalogOpen] = useState(false);
  const [sourcesOpen, setSourcesOpen] = useState(false);
  const [actionError, setActionError] = useState<SkinPackErrorCode | null>(null);
  const [actionWarn, setActionWarn] = useState<"ffmpeg_unavailable" | null>(null);

  useEffect(() => subscribeAppearanceWriteBusy(setWriteBusy), []);

  const reload = useCallback(async () => {
    if (!desktop) return;
    const r = await skinPresetList();
    setPresets(r.presets);
    setUsage(r.usage);
  }, [desktop]);

  useEffect(() => {
    void reload();
    return subscribeSkinLibraryChanged(() => {
      void reload();
    });
  }, [reload]);

  useEffect(() => {
    let cancelled = false;
    void (async () => {
      await ensureMediaEndpoint();
      if (cancelled) return;
      const next: Record<string, string> = {};
      await Promise.all(
        presets.map(async (p) => {
          const src = await resolvePresetThumbSrc(p);
          if (src) next[p.id] = src;
        }),
      );
      if (!cancelled) setThumbSrcById(next);
    })();
    return () => {
      cancelled = true;
    };
  }, [presets]);

  const activeId = resolveActivePresetId(loadActivePresetId(), presets, {
    skin: theme.skin,
    wallpaperRecord: theme.wallpaperRecord,
    wallpaperScrim: theme.wallpaperScrim,
  });

  const locked = busy || writeBusy || share.appearanceBusy;
  const showOfficialBrowse = officialCatalogConfigured();

  const runNamed = useCallback(async () => {
    const name = nameValue.trim();
    if (!name) return;
    setBusy(true);
    setActionError(null);
    setActionWarn(null);
    try {
      if (nameOpen === "save") {
        const manifest = currentLookManifest({
          name,
          skin: theme.skin,
          scrim: theme.wallpaperScrim,
          wallpaper: theme.wallpaperRecord,
          textColor: theme.textColor,
          fontShadow: theme.fontShadow,
        });
        let stagingId: string | null = null;
        if (theme.wallpaperRecord?.blob) {
          stagingId = await uploadCurrentWallpaper({
            blob: theme.wallpaperRecord.blob,
          });
        }
        const entry = await skinPresetSaveFromUpload(stagingId ?? "", manifest);
        saveActivePresetId(entry.id);
        notifySkinLibraryChanged();
        await reload();
      } else if (nameOpen === "export") {
        const dest = await skinPickSave(exportFileName(name));
        if (!dest) return;
        const manifest = currentLookManifest({
          name,
          skin: theme.skin,
          scrim: theme.wallpaperScrim,
          wallpaper: theme.wallpaperRecord,
          textColor: theme.textColor,
          fontShadow: theme.fontShadow,
        });
        let stagingId: string | null = null;
        if (theme.wallpaperRecord?.blob) {
          stagingId = await uploadCurrentWallpaper({
            blob: theme.wallpaperRecord.blob,
          });
        }
        const exported = await skinPackExport(dest, stagingId, manifest);
        if (exported.warning === "ffmpeg_unavailable") {
          setActionWarn("ffmpeg_unavailable");
        }
      } else if (nameOpen === "rename" && renameId) {
        await skinPresetRename(renameId, name);
        notifySkinLibraryChanged();
        await reload();
      }
    } catch (e) {
      setActionError(parseSkinPackError(e).code);
    } finally {
      setBusy(false);
      setNameOpen(null);
      setRenameId(null);
    }
  }, [nameOpen, nameValue, reload, renameId, theme]);

  const runUpdate = useCallback(async () => {
    if (!updateId) return;
    setBusy(true);
    setActionError(null);
    setActionWarn(null);
    try {
      const target = presets.find((p) => p.id === updateId);
      const manifest = currentLookManifest({
        name: target?.name ?? "skin",
        skin: theme.skin,
        scrim: theme.wallpaperScrim,
        wallpaper: theme.wallpaperRecord,
        textColor: theme.textColor,
        fontShadow: theme.fontShadow,
      });
      let stagingId = "";
      if (theme.wallpaperRecord?.blob) {
        stagingId = await uploadCurrentWallpaper({
          blob: theme.wallpaperRecord.blob,
        });
      }
      const entry = await skinPresetReplaceFromUpload(
        updateId,
        stagingId,
        manifest,
      );
      saveActivePresetId(entry.id);
      notifySkinLibraryChanged();
      await reload();
    } catch (e) {
      setActionError(parseSkinPackError(e).code);
    } finally {
      setBusy(false);
      setUpdateId(null);
    }
  }, [presets, reload, theme, updateId]);

  if (!desktop) return null;

  return (
    <div
      className="settings-card skin-presets-card"
      id="settings-anchor-skin-presets"
    >
      <div className="skin-presets-card__head">
        <div className="settings-label">{t("settings.skinPresets.title")}</div>
        <div className="skin-presets__actions">
          <Tip label={t("settings.skinPresets.saveCurrent")}>
            <button
              type="button"
              className="chrome-btn"
              disabled={locked}
              aria-label={t("settings.skinPresets.saveCurrent")}
              onClick={() => {
                setNameValue("");
                setNameOpen("save");
              }}
            >
              <IconPlus size={15} />
            </button>
          </Tip>
          <Tip label={t("settings.skinPresets.importFile")}>
            <button
              type="button"
              className="chrome-btn"
              disabled={locked}
              aria-label={t("settings.skinPresets.importFile")}
              onClick={() => {
                void (async () => {
                  const path = await skinPickOpen();
                  if (path) await share.openFilePreview(path);
                })();
              }}
            >
              <IconUpload size={15} />
            </button>
          </Tip>
          <Tip label={t("settings.skinPresets.exportCurrent")}>
            <button
              type="button"
              className="chrome-btn"
              disabled={locked}
              aria-label={t("settings.skinPresets.exportCurrent")}
              onClick={() => {
                setNameValue("");
                setNameOpen("export");
              }}
            >
              <IconExportImage size={15} />
            </button>
          </Tip>
          {showOfficialBrowse ? (
            <Tip label={t("settings.skinCatalog.browse")}>
              <button
                type="button"
                className="chrome-btn"
                id="settings-anchor-skin-catalog"
                disabled={locked}
                aria-label={t("settings.skinCatalog.browse")}
                onClick={() => setCatalogOpen(true)}
              >
                <IconSparkles size={15} />
              </button>
            </Tip>
          ) : (
            <span id="settings-anchor-skin-catalog" className="sr-only">
              {t("settings.skinCatalog.browse")}
            </span>
          )}
          <Tip label={t("settings.skinCatalog.manageSources")}>
            <button
              type="button"
              className="chrome-btn"
              id="settings-anchor-skin-sources"
              disabled={locked}
              aria-label={t("settings.skinCatalog.manageSources")}
              onClick={() => setSourcesOpen(true)}
            >
              <IconFolder size={15} />
            </button>
          </Tip>
          {usage.hasUndo ? (
            <Tip label={t("settings.skinPresets.undoLast")}>
              <button
                type="button"
                className="chrome-btn"
                disabled={locked}
                aria-label={t("settings.skinPresets.undoLast")}
                onClick={() =>
                  void share.openPresetPreview("before-last-apply", true)
                }
              >
                <IconRewind size={15} />
              </button>
            </Tip>
          ) : null}
        </div>
      </div>
      {presets.length === 0 ? (
        <p className="settings-desc skin-presets-card__empty">
          {t("settings.skinPresets.empty")}
        </p>
      ) : (
        <ul className="skin-presets__list">
          {presets.map((p) => (
            <li key={p.id}>
              <div
                className={
                  "skin-presets__card" +
                  (p.id === activeId ? " is-current" : "") +
                  (locked ? " is-disabled" : "")
                }
                style={presetCardStyle(p, thumbSrcById[p.id])}
                role="button"
                tabIndex={locked ? -1 : 0}
                aria-pressed={p.id === activeId}
                aria-label={p.name}
                onClick={() => {
                  if (!locked) void share.openPresetPreview(p.id);
                }}
                onKeyDown={(e) => {
                  if (locked) return;
                  if (e.key === "Enter" || e.key === " ") {
                    e.preventDefault();
                    void share.openPresetPreview(p.id);
                  }
                }}
              >
                <span className="skin-presets__card-name">{p.name}</span>
                <div
                  className="skin-presets__card-actions"
                  onClick={(e) => e.stopPropagation()}
                  onKeyDown={(e) => e.stopPropagation()}
                >
                  <Tip label={t("settings.skinPresets.apply")}>
                    <button
                      type="button"
                      className="chrome-btn"
                      disabled={locked}
                      aria-label={t("settings.skinPresets.apply")}
                      onClick={() => void share.openPresetPreview(p.id)}
                    >
                      <IconCheck size={14} />
                    </button>
                  </Tip>
                  <Tip label={t("settings.skinPresets.updateCurrent")}>
                    <button
                      type="button"
                      className="chrome-btn"
                      disabled={locked}
                      aria-label={t("settings.skinPresets.updateCurrent")}
                      onClick={() => setUpdateId(p.id)}
                    >
                      <IconRefresh size={14} />
                    </button>
                  </Tip>
                  <Tip label={t("settings.skinPresets.rename")}>
                    <button
                      type="button"
                      className="chrome-btn"
                      disabled={locked}
                      aria-label={t("settings.skinPresets.rename")}
                      onClick={() => {
                        setRenameId(p.id);
                        setNameValue(p.name);
                        setNameOpen("rename");
                      }}
                    >
                      <IconRename size={14} />
                    </button>
                  </Tip>
                  <Tip label={t("settings.skinPresets.export")}>
                    <button
                      type="button"
                      className="chrome-btn"
                      disabled={locked}
                      aria-label={t("settings.skinPresets.export")}
                      onClick={() => {
                        void (async () => {
                          setActionError(null);
                          setActionWarn(null);
                          try {
                            const dest = await skinPickSave(
                              exportFileName(p.name),
                            );
                            if (!dest) return;
                            const exported = await skinPresetExport(p.id, dest);
                            if (exported.warning === "ffmpeg_unavailable") {
                              setActionWarn("ffmpeg_unavailable");
                            }
                          } catch (e) {
                            setActionError(parseSkinPackError(e).code);
                          }
                        })();
                      }}
                    >
                      <IconExportImage size={14} />
                    </button>
                  </Tip>
                  <Tip label={t("settings.skinPresets.delete")}>
                    <button
                      type="button"
                      className="chrome-btn"
                      disabled={locked}
                      aria-label={t("settings.skinPresets.delete")}
                      onClick={() => setDeleteId(p.id)}
                    >
                      <IconTrash size={14} />
                    </button>
                  </Tip>
                </div>
              </div>
            </li>
          ))}
        </ul>
      )}
      {actionError || share.notice?.kind === "err" ? (
        <p className="settings-wallpaper__error" role="alert">
          {t(
            `settings.skinPack.err.${actionError ?? share.notice!.code}` as "settings.skinPack.err.busy",
          )}
        </p>
      ) : actionWarn ? (
        <p className="settings-desc" role="status">
          {t("settings.skinPack.warn.ffmpeg_unavailable")}
        </p>
      ) : share.notice?.kind === "warn" ? (
        <p className="settings-desc" role="status">
          {t(
            `settings.skinPack.warn.${share.notice.code}` as "settings.skinPack.warn.unknown_skin",
          )}
        </p>
      ) : null}

      <GlassModal
        open={!!nameOpen}
        onClose={() => setNameOpen(null)}
        title={
          nameOpen === "rename"
            ? t("settings.skinPresets.renameTitle")
            : t("settings.skinPresets.nameTitle")
        }
        wrapBody
        footer={
          <>
            <button type="button" className="btn btn--ghost" onClick={() => setNameOpen(null)}>
              {t("common.cancel")}
            </button>
            <button type="button" className="btn btn--solid" disabled={busy} onClick={() => void runNamed()}>
              {t("common.save")}
            </button>
          </>
        }
      >
        <label className="skin-sources__add">
          <span className="skin-sources__add-label">
            {t("settings.skinPresets.nameLabel")}
          </span>
          <input
            className="settings-input"
            value={nameValue}
            maxLength={80}
            onChange={(e) => setNameValue(e.target.value)}
          />
        </label>
      </GlassModal>

      <GlassModal
        open={!!deleteId}
        onClose={() => setDeleteId(null)}
        title={t("settings.skinPresets.deleteTitle")}
        wrapBody
        footer={
          <>
            <button type="button" className="btn btn--ghost" onClick={() => setDeleteId(null)}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="btn btn--solid"
              onClick={() => {
                if (!deleteId) return;
                void skinPresetDelete(deleteId).then(() => {
                  if (loadActivePresetId() === deleteId) {
                    saveActivePresetId(null);
                  }
                  setDeleteId(null);
                  notifySkinLibraryChanged();
                  void reload();
                });
              }}
            >
              {t("settings.skinPresets.delete")}
            </button>
          </>
        }
      >
        <p>{t("settings.skinPresets.deleteConfirm")}</p>
      </GlassModal>

      <GlassModal
        open={!!updateId}
        onClose={() => setUpdateId(null)}
        title={t("settings.skinPresets.updateTitle")}
        wrapBody
        footer={
          <>
            <button type="button" className="btn btn--ghost" onClick={() => setUpdateId(null)}>
              {t("common.cancel")}
            </button>
            <button
              type="button"
              className="btn btn--solid"
              disabled={busy}
              onClick={() => void runUpdate()}
            >
              {t("settings.skinPresets.updateCurrent")}
            </button>
          </>
        }
      >
        <p>{t("settings.skinPresets.updateConfirm")}</p>
      </GlassModal>

      <SkinCatalogModal open={catalogOpen} onClose={() => setCatalogOpen(false)} />
      <SkinSourcesModal open={sourcesOpen} onClose={() => setSourcesOpen(false)} />
    </div>
  );
}

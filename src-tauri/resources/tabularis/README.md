# Tabularis sidecar

`scripts/build-local.sh` copies a headless `tabularis` / `tabularis.exe` here before packaging.
If this folder only contains this note, AtlasBuddy still starts when it finds the program through
`ATLAS_TABULARIS_BIN`, Settings → Database, a sibling `tabularis` release build, or `PATH`.

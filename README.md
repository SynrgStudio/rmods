# rmods

GitHub-hosted rMenu module registry.

## Layout

```text
rmods/
  modules/
    example.rmod
  rpacks/
    shortcuts/
      module.toml
      module.js
      config.json
      README.md
  registry.json
  scripts/
    generate-registry.py
```

`registry.json` is generated from `modules/*.rmod` and `rpacks/*`. Do not edit it by hand.

## Package kinds

- `rmod`: single-file UTF-8 text module.
- `rpack`: folder module for multi-file modules, helpers, scripts, assets, config, and docs.

An `rpack` is a folder, not a zip/archive.

Some rpacks declare a `[resident]` helper in `module.toml`. `rmenu-daemon` starts/stops those helpers after install and on daemon lifecycle events. Resident helpers may use OS integrations such as low-level hooks, so their README should document exact behavior and security implications.

Current resident rpacks:

- `taskbar-volume`: wheel/middle-click volume control over the Windows taskbar.
- `thorium-tabs`: Alt+mouse tab gestures when Thorium is active.

## Generate registry locally

```powershell
python .\scripts\generate-registry.py
```

The generator validates every `.rmod` and every `rpack`, extracts metadata, computes SHA-256/size integrity data, and writes deterministic `registry.json` records.

Default raw URLs point to:

```text
https://raw.githubusercontent.com/SynrgStudio/rmods/main/modules/<file>.rmod
https://raw.githubusercontent.com/SynrgStudio/rmods/main/rpacks/<module>/<file>
```

## Add a single-file module

1. Copy a valid `.rmod` into `modules/`.
2. Run `python .\scripts\generate-registry.py`.
3. Commit the `.rmod` and generated `registry.json`.

## Add a folder module

1. Create `rpacks/<id>/module.toml`.
2. Add the entry file and supporting files next to it.
3. For resident helpers, place the native helper inside the rpack folder, usually under `bin/`, and declare it with `[resident]`.
4. Run `python .\scripts\generate-registry.py`.
5. Commit the folder and generated `registry.json`.

The `Update rMods registry` GitHub Action runs the generator automatically when `modules/**`, `rpacks/**`, `scripts/generate-registry.py`, or the workflow file changes. If generated `registry.json` differs, the action commits it with the GitHub Actions bot. If it is unchanged, the action exits without creating an empty commit.

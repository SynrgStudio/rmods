# rmods

GitHub-hosted rMenu `.rmod` registry.

## Layout

```text
rmods/
  modules/
    example.rmod
  registry.json
  scripts/
    generate-registry.py
```

`registry.json` is generated from `modules/*.rmod`. Do not edit it by hand.

## Generate registry locally

```powershell
python .\scripts\generate-registry.py
```

The generator validates every `.rmod`, extracts header metadata, computes SHA-256 and size, and writes a deterministic `registry.json`.

Default download URLs point to:

```text
https://raw.githubusercontent.com/SynrgStudio/rmods/main/modules/<file>.rmod
```

## Add a module

1. Copy a valid `.rmod` into `modules/`.
2. Run `python .\scripts\generate-registry.py`.
3. Commit the `.rmod` and generated `registry.json`.

A GitHub Action will be added to run the same generator automatically on push.

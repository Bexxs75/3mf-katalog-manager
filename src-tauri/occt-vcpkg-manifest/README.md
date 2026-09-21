# OCCT vcpkg manifest (Windows)

`vcpkg.json` in this directory pins OCCT to 7.9.3 (dynamic `x64-windows`
triplet), matching the version verified in
`src-tauri/vendor/opencascade-sys/VENDORING.md`. It exists as a manifest root
for `vcpkg install`, used both locally and by
`.github/workflows/build-windows-step.yml`.

This is a plain JSON manifest, not a binary — committing it is fine and keeps
the CI workflow and local dev instructions in sync instead of duplicating the
version pin in prose.

Local usage:

```powershell
cd src-tauri\occt-vcpkg-manifest
vcpkg install --triplet x64-windows
```

The resulting `vcpkg_installed\x64-windows` directory is what
`src-tauri\scripts\stage-occt-dlls.ps1` reads from.

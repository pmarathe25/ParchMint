# Localization

> Read when adding or changing user-visible text.

- QML text uses `qsTr`; C++ text uses `tr`.
- Rust emits stable machine codes with localizable text at the application boundary.
- English is the source language.
- `translations/ParchMint_en.ts` is the reviewed source-string inventory.

## Update a catalog

1. Run Qt `lupdate` over QML and C++ sources.
2. Review added, removed, and changed source strings; preserve accelerator markers.
3. Compile catalogs with the normal CMake build (`lrelease`).
4. Check truncation, wrapping, RTL, mixed scripts, and 100/150/200% scaling.

CMake embeds `translations/*.qm` under `:/i18n`; `main.cpp` selects the system
locale and falls back to source strings. Physical input, layout, and screen-reader
checks follow [release validation](../release/platform-validation.md).

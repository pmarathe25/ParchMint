# Translation catalog and string expansion checks

User-visible QML strings use `qsTr`; Rust diagnostics and exporter warnings
use stable machine codes with localizable message text at the application
boundary. English remains the source language. Translators must work from the
source string and context, never from an inferred translation.

Before adding a release catalog:

1. Run Qt `lupdate` over `app/qml` and the C++ bridge sources.
2. Review every new source string and preserve accelerator markers.
3. Check menu width, binder rows, dialogs, warnings, and the formatting bar at
   100%, 150%, and 200% display scale.
4. Record truncation, wrapping, RTL, and mixed-script observations in the
   platform charter.

`translations/ParchMint_en.ts` is the checked-in source-language catalog for
the current UI surface. Run `lupdate` before adding or removing strings and
review the diff; it is a source inventory, not a guessed non-English
translation. Stage 09 owns signed locale catalogs and final screen-reader
review.

# Release candidate checklist

No checkbox may be marked from inference. Attach CI URLs, artifact hashes, and
physical charter records to the Stage 09 handoff.

- [ ] ADR-0013 accepted by the repository owner; Qt obligations reviewed.
- [ ] Clean tagged builds pass locked tests, lint, parser fuzz smoke, benchmarks,
      dependency licenses/advisories, SBOM/notices, and package inspection.
- [ ] Windows MSI/ZIP install, upgrade, association, uninstall, signature, icon,
      Narrator, IME, high-DPI, sleep/resume, failure injection, and consumer export.
- [ ] macOS app/DMG install, upgrade, association, uninstall, Gatekeeper,
      notarization, VoiceOver, IME, scaling, sleep/resume, and consumer export.
- [ ] Linux AppImage and Flatpak/TGZ install/uninstall, desktop/MIME integration,
      Orca, IME, Wayland/X11, scaling, failure injection, and consumer export.
- [ ] Cold launch, stress open, search, editing latency, scroll FPS, memory, file
      handles, threads, compile, export, and repeated open/close meet budgets.
- [ ] Fresh install and every prerelease schema preserve projects; `.parchmint/`
      deletion and application uninstall leave canonical projects complete.
- [ ] Long editing/autosave and repeated abrupt recovery runs pass.
- [ ] No P0/P1 correctness, data-loss, security, accessibility, or packaging bug.
- [ ] Publication is explicitly authorized through the protected release environment.

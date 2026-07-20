# Windows packaging

CPack's WiX generator owns install/uninstall and version metadata. A checked-in
multiresolution `.ico` is still required before a release candidate can pass
the Windows visual-branding gate. Signing is performed only by the authorized
release workflow with an injected certificate; keys never belong here.

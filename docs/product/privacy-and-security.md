# Privacy and security

- **SEC-001:** Core functionality requires no network connection.
- **SEC-002:** The application loads and executes only trusted, bundled application content in v1. Application views must not navigate to remote origins; validated user links open through the external-open platform service.
- **SEC-003:** The application must enforce strict content and execution rules. Privileged operations use least-privilege application and platform-service boundaries bound to the originating window and project session.
- **SEC-004:** No remote content receives privileged application access.
- **SEC-005:** Project paths and pasted HTML are validated/sanitized.
- **SEC-006:** History and search network features are disabled in v1.
- **SEC-007:** Dependency locks, advisories, license inventory, provenance checks, and SBOM are release artifacts.
- **SEC-008:** Spellcheck dictionaries and language data are bundled or otherwise available offline under compatible licenses; user prose is not sent to a network service.

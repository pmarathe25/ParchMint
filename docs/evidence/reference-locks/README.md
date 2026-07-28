# Reference locks

These are exact copies from the V02-R evidence package.

They establish the audited Tauri/ProseMirror dependency graph used by the control prototype. They are **reference/provenance files**, not drop-in final application lockfiles. The implementation will add React and application dependencies and therefore produce new committed locks.

Preserve the validated ProseMirror package versions initially unless an ADR approves an upgrade.

Expected SHA-256:

- `v02-cargo.lock`: `c459d31ef0717bde10fd366a4151d4f781984284dc33d135c41d9eadea51f2c9`
- `v02-package-lock.json`: `43adb95f615d22d073973b54d5e6cfec5ac96e350edd454c0cd74158f9a71a83`

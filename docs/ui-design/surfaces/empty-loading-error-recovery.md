# Empty, loading, error, and recovery states

Empty, loading, error, and recovery states retain normal shell and pane context;
use nested surfaces only for warnings, confirmations, and remediation.

Empty, loading, error, and recovery states use the full available content area
with centered outer margins. States inside an editor keep the shell, pane, and
tab context. Use a nested surface for a warning, recovery choice, destructive
confirmation, or remediation, and keep centered messages clear of clipping.
ParchMint works offline, so an offline warning is unnecessary.

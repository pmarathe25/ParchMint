# ParchMint privacy statement

ParchMint version 1 is local-first and has no account, telemetry, analytics,
advertising, crash-upload, update-check, or other network feature. Project text,
metadata, research, attachments, search indexes, backups, recovery records,
settings, and logs remain on the user's devices and chosen storage locations.

The application writes local JSON-lines runtime logs to the operating system's
application-data directory. A diagnostics export happens only after the user
chooses the command and destination. The exported report contains application,
OS and architecture versions, node counts, and non-content warnings; it omits
project paths and writing. ParchMint never transmits the report.

Opening an attachment in another application is a separately confirmed action.
That application has its own privacy and security behavior. Files stored in a
cloud-synchronized or network folder are handled by that storage provider, not
by ParchMint.

To erase local derived state, remove the platform ParchMint application-data
directory and the project's `.parchmint/` directory. Doing so does not erase the
project's canonical Markdown, TOML, assets, or recoverable `trash/` data.

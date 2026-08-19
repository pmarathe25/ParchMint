# `parchmint-diagnostics`

`parchmint-diagnostics` provides the desktop application's local,
dependency-free runtime log. Production configures it below the platform data
directory at `logs/parchmint-debug.log`; before that point it writes to standard
error so startup failures are still observable.

The log is line-oriented. Each event has a timestamp, an in-process sequence
number, level, component, operation, and safe fields. Callers must not log
document text. The logger is best-effort, so an unavailable log file does not
change application behavior.

# ADR-0006: Atomic canonical replacement and recovery direction

Status: Accepted (Stage 01)

## Decision

Write a uniquely named temporary file in the destination directory, write all
bytes, flush the file, atomically replace the destination, then flush directory
metadata where the platform exposes a normal directory handle. Report “saved”
only after this sequence succeeds. Never remove the destination before rename.

Stage 03 will journal revisioned changes before scheduling canonical replacement;
Stage 02 migrations will back up canonical inputs before mutation. Windows does
not provide a portable ordinary-user directory `fsync`; replacement plus file
flush is the documented platform boundary and must receive failure-injection
coverage on Windows CI.

# Assistant API

Provider-neutral requests, responses, evidence, errors, and explicitly
authorized tools for the project assistant. This crate performs no project
mutations or network I/O. Credentials have redacted debug output. A caller
must authorize note access and revalidate any requested action before applying
it. Source IDs are local to one request.

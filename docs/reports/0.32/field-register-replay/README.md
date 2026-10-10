# Fixed Cell field/register replay

Storage note (2026-10-10): this historical replay's JSON payloads are retained
in the [pinned Git archive](../README.md#historical-json-storage). The original
manifest and logs remain here; restore archived payloads before verifying the
complete manifest. The following results retain their original source scope.


These raw reports bind clean signed `588f6a28`, which is public on `0.32`.
The dev run used the exact staged sources later signed as that commit; it was
not a clean-HEAD replay. The backend, acceptance and node reports bind clean
`588f6a28`, not the later containing archive commit. Gzip mtime is normalized,
and the manifest retains raw/stored sizes and SHA-256 digests.

The pinned CKB backend and three confirmed CCC fee-signed transitions passed.
Counter setup remains a local public test seed. These reports do not complete
H1/H2, peer execution, general migration, production setup or stable admission.
Full CI remains deferred until the complete requested queue is implemented.

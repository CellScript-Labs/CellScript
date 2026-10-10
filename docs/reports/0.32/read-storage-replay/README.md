# Clean fixed read/storage replay

Storage note (2026-10-10): this historical replay's JSON payloads are retained
in the [pinned Git archive](../README.md#historical-json-storage). The original
manifest and logs remain here; restore archived payloads before verifying the
complete manifest. The following results retain their original source scope.


The manifest binds signed source commit
`f07b8a1cb7451ad33f2b14b9773b09f473fca2aa`, its clean root backend/CCC replay
and its earlier exact staged dev state. It preserves seven gzip files with
zero timestamps and raw/stored sizes plus SHA-256 hashes. The containing
archive commit is separate from the source commit validated by these reports.

The local-node rows confirmed old update, old-key-authorized migration and new
successor update using actual fee signatures: 111,742,208 / 111,890,107 /
111,741,254 cycles and 1237 / 1632 / 1237 bytes. The node and acceptance reports
bind `git_dirty=false`, the exact source commit and tracked-source SHA-256.
Signed fixed read/storage commits were pushed to `public/0.32`.

This is a disposable local chain with public test setup for the successor. It
does not complete general #45 lifecycle admission, authorize production setup,
establish public deployment or certify later field-codec changes. Earlier
`23822919` pool snapshot replay remains in its separate archive. Complete CI
is deferred until the whole implementation queue is complete.

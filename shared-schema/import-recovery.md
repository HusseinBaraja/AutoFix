# Settings import recovery protocol

WPF and the Rust settings feature coordinate using a non-shared Windows handle
to `settings.toml.lock` in the product directory. Every live settings read and
write owns this handle. A terminated process releases it automatically. The
engine denies correction admission, outbound API sends and replacement whenever
`settings_import_recovery` in `autofix.sqlite` contains any record. Missing legacy
tables are allowed; unreadable recovery state denies correction.

The UI initializes this recovery table without changing engine migration versions:

```sql
create table settings_import_recovery (
    id integer primary key check (id = 1),
    original_settings blob not null,
    imported_settings blob not null,
    committed integer not null check (committed in (0, 1))
);
```

An import first validates the preview against settings and participating product
rows under the settings lock and a SQLite write transaction. That transaction
durably inserts the two exact TOML byte snapshots and `committed = 0` using
SQLite synchronous FULL. Only after that commit may the settings file change.
The second transaction rechecks participating product data, replaces those rows,
and sets `committed = 1`. The flushed temporary TOML replaces the live file using
Windows `MoveFileExW(REPLACE_EXISTING | WRITE_THROUGH)` before this second commit.

After interruption, settings access takes the same lock and a SQLite writer
reservation. A zero record restores original bytes; a one record restores imported
bytes. The chosen bytes are flushed and atomically replace the live file before
the journal record is deleted and that deletion commits with synchronous FULL.
A missing settings file is restored rather than replaced with defaults. Another
interruption repeats the same decision, so recovery is idempotent. Any failure
leaves the journal intact, keeps correction blocked and reports the underlying
error. The engine retries config reload failures instead of consuming the file's
new modification timestamp. Stale saves which trigger recovery require a reload;
IPC setting edits read and update the latest recovered configuration under the lock.

The journal is strictly local recovery storage. It is excluded from bundles,
along with lock files, temporary settings files and retained error backups.
Durability depends on the filesystem and device honoring Windows and SQLite
flush requests; physical storage corruption is not recoverable by this protocol.

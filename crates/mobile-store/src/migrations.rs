use rusqlite::{Connection, TransactionBehavior};

use crate::StoreError;

const MIGRATIONS: &[(i64, &str)] = &[
    (
        1,
        r#"
        CREATE TABLE cached_entities (
          kind TEXT NOT NULL,
          id TEXT NOT NULL,
          org_id TEXT,
          status TEXT,
          payload_json TEXT NOT NULL,
          server_updated_at INTEGER,
          cached_at INTEGER NOT NULL,
          PRIMARY KEY (kind, id)
        );
        CREATE INDEX idx_cached_entities_org_kind
          ON cached_entities(org_id, kind, cached_at DESC);
        CREATE INDEX idx_cached_entities_kind_status
          ON cached_entities(kind, status, cached_at DESC);

        CREATE TABLE gps_fixes (
          id TEXT PRIMARY KEY NOT NULL,
          trip_id TEXT,
          lat REAL NOT NULL,
          lng REAL NOT NULL,
          speed REAL,
          heading REAL,
          accuracy REAL,
          recorded_at INTEGER NOT NULL,
          uploaded_at INTEGER,
          rejected_at INTEGER,
          reject_reason TEXT
        );
        CREATE INDEX idx_gps_fixes_pending
          ON gps_fixes(uploaded_at, rejected_at, recorded_at);

        CREATE TABLE eld_records (
          device_id TEXT NOT NULL,
          sequence INTEGER NOT NULL,
          recorded_at INTEGER NOT NULL,
          record_type TEXT NOT NULL,
          payload_json TEXT NOT NULL,
          received_at INTEGER NOT NULL,
          uploaded_at INTEGER,
          rejected_at INTEGER,
          reject_reason TEXT,
          PRIMARY KEY (device_id, sequence)
        );
        CREATE INDEX idx_eld_records_pending
          ON eld_records(uploaded_at, rejected_at, recorded_at, sequence);

        CREATE TABLE mutation_outbox (
          operation_id TEXT PRIMARY KEY NOT NULL,
          mutation_name TEXT NOT NULL,
          entity_kind TEXT,
          entity_id TEXT,
          org_id TEXT,
          payload_json TEXT NOT NULL,
          state TEXT NOT NULL,
          attempt_count INTEGER NOT NULL DEFAULT 0,
          next_attempt_at INTEGER NOT NULL,
          lease_owner TEXT,
          lease_expires_at INTEGER,
          last_error_code TEXT,
          last_error_message TEXT,
          created_at INTEGER NOT NULL,
          updated_at INTEGER NOT NULL,
          acknowledged_at INTEGER
        );
        CREATE INDEX idx_mutation_outbox_ready
          ON mutation_outbox(state, next_attempt_at, created_at);
        CREATE INDEX idx_mutation_outbox_entity
          ON mutation_outbox(entity_kind, entity_id, created_at);

        CREATE TABLE sync_checkpoints (
          stream TEXT PRIMARY KEY NOT NULL,
          cursor TEXT,
          server_version TEXT,
          updated_at INTEGER NOT NULL
        );
        "#,
    ),
    (
        2,
        r#"
        ALTER TABLE mutation_outbox RENAME TO function_outbox;
        ALTER TABLE function_outbox RENAME COLUMN mutation_name TO function_name;
        ALTER TABLE function_outbox
          ADD COLUMN function_kind TEXT NOT NULL DEFAULT 'mutation';

        DROP INDEX IF EXISTS idx_mutation_outbox_ready;
        DROP INDEX IF EXISTS idx_mutation_outbox_entity;
        CREATE INDEX idx_function_outbox_ready
          ON function_outbox(state, next_attempt_at, created_at);
        CREATE INDEX idx_function_outbox_entity
          ON function_outbox(entity_kind, entity_id, created_at);
        "#,
    ),
    (
        3,
        r#"
        CREATE TABLE query_snapshots (
          subscription_key TEXT PRIMARY KEY NOT NULL,
          function_name TEXT NOT NULL,
          args_json TEXT NOT NULL,
          result_json TEXT NOT NULL,
          revision INTEGER NOT NULL,
          server_updated_at INTEGER,
          cached_at INTEGER NOT NULL
        );
        CREATE INDEX idx_query_snapshots_function
          ON query_snapshots(function_name, cached_at DESC);
        "#,
    ),
];

pub(crate) fn migrate(connection: &mut Connection) -> Result<(), StoreError> {
    connection.execute_batch(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
           version INTEGER PRIMARY KEY NOT NULL,
           applied_at INTEGER NOT NULL
         );",
    )?;
    for (version, sql) in MIGRATIONS {
        let exists: bool = connection.query_row(
            "SELECT EXISTS(SELECT 1 FROM schema_migrations WHERE version = ?1)",
            [version],
            |row| row.get(0),
        )?;
        if exists {
            continue;
        }
        let transaction = connection.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(sql)?;
        transaction.execute(
            "INSERT INTO schema_migrations(version, applied_at)
             VALUES (?1, CAST(unixepoch('subsec') * 1000 AS INTEGER))",
            [version],
        )?;
        transaction.commit()?;
    }
    Ok(())
}

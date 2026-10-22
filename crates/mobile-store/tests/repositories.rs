//! Local repository and outbox behavior.

use mobile_models::OutboxState;
use mobile_store::{
    CachedEntity, CachedEntityRepository, DatabaseKey, EldRecord, EldRepository, GpsFix,
    GpsRepository, MobileStore, NewOutboxItem, OutboxRepository, QuerySnapshotRepository,
};
use serde_json::json;

#[test]
fn caches_server_entities() {
    let store = store();
    let repository = CachedEntityRepository::new(&store);
    repository
        .upsert(&CachedEntity {
            kind: "task".to_owned(),
            id: "task-1".to_owned(),
            org_id: Some("org-1".to_owned()),
            status: Some("open".to_owned()),
            payload: json!({"_id": "task-1", "status": "open"}),
            server_updated_at: Some(100),
            cached_at: 110,
        })
        .expect("entity should cache");
    let loaded = repository
        .get("task", "task-1")
        .expect("entity should load")
        .expect("entity should exist");
    assert_eq!(loaded.payload["status"], "open");
}

#[test]
fn leases_retries_and_acknowledges_outbox_work() {
    let store = store();
    let repository = OutboxRepository::new(&store);
    let item = repository
        .enqueue(NewOutboxItem {
            operation_id: None,
            function_name: "tasks:create".to_owned(),
            entity_kind: Some("task".to_owned()),
            entity_id: Some("task-1".to_owned()),
            org_id: Some("org-1".to_owned()),
            payload: json!({"title": "Inspect pump"}),
            created_at: 1_000,
        })
        .expect("item should enqueue");
    let claimed = repository
        .claim_ready(1_000, 10, "worker-1", 30_000)
        .expect("item should claim");
    assert_eq!(claimed.len(), 1);
    assert_eq!(claimed[0].state, OutboxState::Sending);

    repository
        .retry(&item.operation_id, 2_000, 5_000, "network", "offline")
        .expect("retry should persist");
    assert!(
        repository
            .claim_ready(4_999, 10, "worker-1", 30_000)
            .expect("query should work")
            .is_empty()
    );
    assert_eq!(
        repository
            .claim_ready(5_000, 10, "worker-1", 30_000)
            .expect("item should become ready")
            .len(),
        1
    );
    repository
        .acknowledge(&item.operation_id, 6_000)
        .expect("ack should persist");
    assert_eq!(
        repository
            .get(&item.operation_id)
            .expect("item should load")
            .expect("item should exist")
            .state,
        OutboxState::Acknowledged
    );
}

#[test]
fn reclaims_work_whose_lease_expired_mid_delivery() {
    let store = store();
    let repository = OutboxRepository::new(&store);
    let item = repository
        .enqueue(NewOutboxItem {
            operation_id: None,
            function_name: "tasks:complete".to_owned(),
            entity_kind: Some("task".to_owned()),
            entity_id: Some("task-1".to_owned()),
            org_id: Some("org-1".to_owned()),
            payload: json!({"taskId": "task-1"}),
            created_at: 1_000,
        })
        .expect("item should enqueue");
    // Worker A claims and then "crashes" without ack/retry/reject.
    assert_eq!(
        repository
            .claim_ready(1_000, 10, "worker-a", 30_000)
            .expect("claim should work")
            .len(),
        1
    );
    // While the lease is live, nobody else may take it.
    assert!(
        repository
            .claim_ready(30_999, 10, "worker-b", 30_000)
            .expect("claim should work")
            .is_empty()
    );
    // Once it expires, another worker reclaims it.
    let reclaimed = repository
        .claim_ready(31_000, 10, "worker-b", 30_000)
        .expect("claim should work");
    assert_eq!(reclaimed.len(), 1);
    assert_eq!(reclaimed[0].operation_id, item.operation_id);
    assert_eq!(reclaimed[0].state, OutboxState::Sending);
}

#[test]
fn rejects_functions_without_server_idempotency() {
    let store = store();
    let error = OutboxRepository::new(&store)
        .enqueue(NewOutboxItem {
            operation_id: None,
            function_name: "tasks:addComment".to_owned(),
            entity_kind: Some("task".to_owned()),
            entity_id: Some("task-1".to_owned()),
            org_id: Some("org-1".to_owned()),
            payload: json!({"taskId": "task-1", "body": "on my way"}),
            created_at: 1_000,
        })
        .expect_err("unsafe operation must not enter the outbox");

    assert!(matches!(
        error,
        mobile_store::StoreError::FunctionNotQueueable(_)
    ));
}

#[test]
fn injects_argument_idempotency_and_rejects_invalid_batches() {
    let store = store();
    let created = OutboxRepository::new(&store)
        .enqueue(NewOutboxItem {
            operation_id: None,
            function_name: "tasks:create".to_owned(),
            entity_kind: Some("task".to_owned()),
            entity_id: None,
            org_id: Some("org-1".to_owned()),
            payload: json!({"orgId": "org-1", "title": "Replace filter"}),
            created_at: 1_000,
        })
        .expect("create should receive an idempotency argument");
    assert_eq!(created.payload["clientId"], created.operation_id.as_str());

    let duplicate_entries = OutboxRepository::new(&store).enqueue(NewOutboxItem {
        operation_id: None,
        function_name: "tasks:logTime".to_owned(),
        entity_kind: Some("task".to_owned()),
        entity_id: Some("task-1".to_owned()),
        org_id: Some("org-1".to_owned()),
        payload: json!({
            "orgId": "org-1",
            "entries": [
                {"entryId": "entry-1", "minutes": 15},
                {"entryId": "entry-1", "minutes": 20}
            ]
        }),
        created_at: 1_000,
    });
    assert!(matches!(
        duplicate_entries,
        Err(mobile_store::StoreError::InvalidIdempotencyArguments(_))
    ));
}

#[test]
fn stores_gps_and_eld_records_idempotently() {
    let store = store();
    let gps = GpsRepository::new(&store);
    let fix = GpsFix {
        id: "fix-1".to_owned(),
        trip_id: Some("trip-1".to_owned()),
        lat: 45.0,
        lng: 7.0,
        speed: Some(10.0),
        heading: Some(90.0),
        accuracy: Some(5.0),
        recorded_at: 1_000,
    };
    gps.insert(&fix).expect("fix should save");
    gps.insert(&fix).expect("duplicate should be ignored");
    assert_eq!(gps.pending(10).expect("fixes should load").len(), 1);
    gps.acknowledge(&["fix-1".to_owned()], 2_000)
        .expect("fix should acknowledge");
    assert!(gps.pending(10).expect("fixes should load").is_empty());

    let eld = EldRepository::new(&store);
    let record = EldRecord {
        device_id: "device-1".to_owned(),
        sequence: 1,
        recorded_at: 1_000,
        record_type: "heartbeat".to_owned(),
        payload: json!({"ok": true}),
    };
    eld.insert_batch(&[record.clone(), record], 1_100)
        .expect("records should save");
    let counts = eld.counts().expect("counts should load");
    assert_eq!(counts.total, 1);
    assert_eq!(counts.pending, 1);
}

#[test]
fn materializes_complete_query_snapshots_with_revisions() {
    let store = store();
    let repository = QuerySnapshotRepository::new(&store);
    let first = repository
        .materialize(
            "open-tasks",
            "tasks:list",
            &json!({"orgId": "org-1"}),
            &json!([{"_id": "task-1"}]),
            None,
            1_000,
        )
        .expect("first snapshot should persist");
    assert_eq!(first.revision, 1);

    let second = repository
        .materialize(
            "open-tasks",
            "tasks:list",
            &json!({"orgId": "org-1"}),
            &json!([{"_id": "task-1"}, {"_id": "task-2"}]),
            None,
            2_000,
        )
        .expect("second snapshot should persist");
    assert_eq!(second.revision, 2);
    assert_eq!(second.result.as_array().expect("array").len(), 2);

    repository
        .remove("open-tasks")
        .expect("snapshot should remove");
    assert!(
        repository
            .get("open-tasks")
            .expect("lookup should work")
            .is_none()
    );
}

fn store() -> MobileStore {
    MobileStore::open(
        ":memory:",
        DatabaseKey::new(&[0x41; 32]).expect("test key should be valid"),
    )
    .expect("store should open")
}

#[test]
fn sensitive_payloads_are_ciphertext_on_disk() {
    let directory = std::env::temp_dir().join(format!(
        "mobile-store-at-rest-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock")
            .as_nanos()
    ));
    std::fs::create_dir_all(&directory).expect("temp dir");
    let path = directory.join("core.sqlite");
    let marker = "PLAINTEXT-MARKER-7f3a";
    {
        let store = MobileStore::open(
            &path,
            DatabaseKey::new(&[0x41; 32]).expect("test key should be valid"),
        )
        .expect("store should open");
        CachedEntityRepository::new(&store)
            .upsert(&CachedEntity {
                kind: "task".to_owned(),
                id: "task-1".to_owned(),
                org_id: None,
                status: None,
                payload: json!({"notes": marker}),
                server_updated_at: None,
                cached_at: 1,
            })
            .expect("entity should cache");
        OutboxRepository::new(&store)
            .enqueue(NewOutboxItem {
                operation_id: None,
                function_name: "tasks:create".to_owned(),
                entity_kind: None,
                entity_id: None,
                org_id: None,
                payload: json!({"title": marker}),
                created_at: 1,
            })
            .expect("item should enqueue");
    }
    for entry in std::fs::read_dir(&directory).expect("dir should list") {
        let bytes = std::fs::read(entry.expect("entry").path()).expect("file should read");
        assert!(
            !bytes
                .windows(marker.len())
                .any(|window| window == marker.as_bytes()),
            "plaintext leaked into a database file"
        );
    }

    let reopened = MobileStore::open(
        &path,
        DatabaseKey::new(&[0x41; 32]).expect("test key should be valid"),
    )
    .expect("store should reopen");
    let entity = CachedEntityRepository::new(&reopened)
        .get("task", "task-1")
        .expect("entity should load")
        .expect("entity should exist");
    assert_eq!(entity.payload["notes"], marker);

    let wrong_key = MobileStore::open(
        &path,
        DatabaseKey::new(&[0x42; 32]).expect("test key should be valid"),
    )
    .expect("store should open with any key");
    assert!(
        CachedEntityRepository::new(&wrong_key)
            .get("task", "task-1")
            .is_err(),
        "a different key must fail authentication"
    );
    drop((reopened, wrong_key));
    let _ = std::fs::remove_dir_all(&directory);
}

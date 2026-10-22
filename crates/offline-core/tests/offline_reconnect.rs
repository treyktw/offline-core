//! End-to-end offline lifecycle through the public facade:
//! enqueue while offline -> lease + backoff -> reconnect -> drain (including a
//! lost acknowledgement) -> realtime materialization of the server's state.

use std::sync::Arc;
use std::time::Duration;

use mobile_sync::RealtimeMaterializerConfig;
use mobile_sync::mock::InMemoryServer;
use offline_core::{
    CompleteTaskIntent, CreateTaskIntent, Environment, LogTimeIntent, OfflineCore,
    OfflineFunctionIntent, RealtimeQueryRequest, SdkConfig, SdkError, SensorBatchIntent,
    SensorReading, TasksSnapshot, TimeEntry,
};
use serde_json::json;

const ORG: &str = "org-demo";
const TASKS_KEY: &str = "tasks.open";

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[allow(clippy::too_many_lines)]
async fn offline_writes_drain_once_and_materialize_after_reconnect() {
    let server = InMemoryServer::new(false);
    let core = OfflineCore::with_transport(
        SdkConfig {
            environment: Environment::Development,
            installation_id: "device-under-test".to_owned(),
            database_path: ":memory:".to_owned(),
            database_key: vec![0x7E; 32],
        },
        Arc::new(server.clone()),
    )
    .expect("core should open")
    .with_realtime_config(RealtimeMaterializerConfig {
        reconnect_delay_ms: 20,
    });

    // The realtime query starts while offline; the materializer keeps retrying.
    core.start_realtime_query(RealtimeQueryRequest {
        subscription_key: TASKS_KEY.to_owned(),
        function_name: "tasks:list".to_owned(),
        args_json: json!({"orgId": ORG}).to_string(),
    })
    .await
    .expect("realtime query should start");

    // --- 1. Offline: every write lands in the encrypted outbox first. ---
    let pump_task = core
        .enqueue_create_task(CreateTaskIntent {
            org_id: ORG.to_owned(),
            title: "Inspect pump".to_owned(),
            created_at: 1_000,
        })
        .expect("create should queue");
    let filter_task = core
        .enqueue_create_task(CreateTaskIntent {
            org_id: ORG.to_owned(),
            title: "Replace filter".to_owned(),
            created_at: 1_001,
        })
        .expect("create should queue");
    core.enqueue_complete_task(CompleteTaskIntent {
        org_id: ORG.to_owned(),
        task_id: pump_task.clone(),
        created_at: 1_002,
    })
    .expect("completion should queue");
    core.enqueue_log_time(LogTimeIntent {
        org_id: ORG.to_owned(),
        task_id: filter_task.clone(),
        entries: vec![
            TimeEntry {
                entry_id: "entry-a".to_owned(),
                minutes: 15,
            },
            TimeEntry {
                entry_id: "entry-b".to_owned(),
                minutes: 30,
            },
        ],
        created_at: 1_003,
    })
    .expect("time log should queue");
    core.enqueue_sensor_batch(SensorBatchIntent {
        org_id: ORG.to_owned(),
        device_id: "sensor-1".to_owned(),
        readings: (0..3)
            .map(|seq| SensorReading {
                seq,
                recorded_at: 1_000 + seq,
                latitude: None,
                longitude: None,
                value: 20.0 + f64::from(u8::try_from(seq).expect("small")),
            })
            .collect(),
        created_at: 1_004,
    })
    .expect("sensor batch should queue");

    // The idempotency gate refuses writes the server cannot deduplicate.
    let comment = core.enqueue_offline_function(OfflineFunctionIntent {
        function_name: "tasks:addComment".to_owned(),
        operation_id: None,
        entity_kind: Some("task".to_owned()),
        entity_id: Some(pump_task.clone()),
        org_id: Some(ORG.to_owned()),
        payload_json: json!({"taskId": pump_task, "body": "fixed"}).to_string(),
        created_at: 1_005,
    });
    assert!(matches!(comment, Err(SdkError::FunctionNotQueueable)));
    assert_eq!(core.status().expect("status").undelivered_operations, 5);

    // --- 2. Drain while offline: everything is leased, fails, and backs off. ---
    let offline_run = core
        .synchronize_outbox_once(10_000)
        .await
        .expect("offline drain should persist retries");
    assert_eq!(offline_run.claimed, 5);
    assert_eq!(offline_run.retry_scheduled, 5);
    assert_eq!(server.stats().write_requests, 0);

    // Before the backoff window opens nothing is claimable.
    let early = core.synchronize_outbox_once(10_500).await.expect("drain");
    assert_eq!(early.claimed, 0);

    // --- 3. Reconnect, but lose the first acknowledgement in flight. ---
    server.set_online(true);
    server.lose_next_acks(1);
    let reconnect_run = core
        .synchronize_outbox_once(20_000)
        .await
        .expect("reconnect drain should run");
    assert_eq!(reconnect_run.claimed, 5);
    assert_eq!(reconnect_run.acknowledged, 4);
    assert_eq!(reconnect_run.retry_scheduled, 1, "lost ack must be retried");
    assert_eq!(core.status().expect("status").undelivered_operations, 1);

    // --- 4. The retry replays the create; the server deduplicates it. ---
    let replay_run = core.synchronize_outbox_once(60_000).await.expect("drain");
    assert_eq!(replay_run.acknowledged, 1);
    assert_eq!(core.status().expect("status").undelivered_operations, 0);

    let stats = server.stats();
    assert_eq!(stats.write_requests, 6, "five operations plus one replay");
    assert_eq!(stats.writes_applied, 5);
    assert_eq!(stats.writes_deduplicated, 1);
    assert_eq!(
        server.tasks().len(),
        2,
        "replay must not duplicate the task"
    );
    assert_eq!(
        server.task(&pump_task).expect("pump task")["status"],
        "done"
    );
    assert_eq!(
        server.task(&filter_task).expect("filter task")["loggedMinutes"],
        45
    );
    assert_eq!(server.reading_count("sensor-1"), 3);

    // --- 5. The realtime materializer reconnected and holds server truth. ---
    let snapshot = wait_for_tasks(&core, |snapshot| {
        snapshot.tasks.len() == 2
            && snapshot
                .tasks
                .iter()
                .any(|task| task.id == pump_task && task.status == "done")
            && snapshot
                .tasks
                .iter()
                .any(|task| task.id == filter_task && task.logged_minutes == 45)
    })
    .await;
    let revision_before_outage = snapshot.revision;

    // --- 6. Another client writes while this device is disconnected. ---
    server.set_online(false);
    server
        .apply_external(
            "tasks:create",
            &json!({"orgId": ORG, "clientId": "task-from-web", "title": "Order parts"}),
        )
        .expect("external write should apply");
    tokio::time::sleep(Duration::from_millis(60)).await;
    let offline_view = core
        .tasks_snapshot(TASKS_KEY.to_owned())
        .expect("snapshot should load")
        .expect("snapshot should exist");
    assert_eq!(
        offline_view.tasks.len(),
        2,
        "the last durable snapshot stays readable offline"
    );

    server.set_online(true);
    let reconnected = wait_for_tasks(&core, |snapshot| snapshot.tasks.len() == 3).await;
    assert!(reconnected.revision > revision_before_outage);
    assert!(server.stats().subscriptions_opened >= 2);

    core.stop_all_realtime().await;
}

async fn wait_for_tasks(
    core: &OfflineCore,
    predicate: impl Fn(&TasksSnapshot) -> bool,
) -> TasksSnapshot {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(snapshot) = core
            .tasks_snapshot(TASKS_KEY.to_owned())
            .expect("snapshot should load")
            && predicate(&snapshot)
        {
            return snapshot;
        }
        assert!(
            tokio::time::Instant::now() < deadline,
            "materialized snapshot never reached the expected state"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

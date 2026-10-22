//! Walks the offline write lifecycle against the in-memory backend and prints
//! what happens at each step.
//!
//! ```text
//! cargo run -p mobile-sync --example offline_demo
//! ```

use std::sync::Arc;
use std::time::Duration;

use mobile_store::{
    DatabaseKey, MobileStore, NewOutboxItem, OutboxRepository, QuerySnapshotRepository,
};
use mobile_sync::mock::InMemoryServer;
use mobile_sync::{
    MaterializationSpec, OutboxWorker, OutboxWorkerConfig, RealtimeMaterializer,
    RealtimeMaterializerConfig,
};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Obviously-fake demo key. Real apps load 32 random bytes from the
    // Keychain / Android Keystore and pass them in.
    let store = Arc::new(MobileStore::open(
        ":memory:",
        DatabaseKey::new(&[0x42; 32]).expect("32-byte demo key"),
    )?);
    let server = InMemoryServer::new(false);
    let outbox = OutboxRepository::new(&store);

    println!("== offline: enqueue ==");
    let create = outbox.enqueue(NewOutboxItem {
        operation_id: None,
        function_name: "tasks:create".to_owned(),
        entity_kind: Some("task".to_owned()),
        entity_id: None,
        org_id: Some("org-demo".to_owned()),
        payload: json!({"orgId": "org-demo", "title": "Inspect pump"}),
        created_at: 1_000,
    })?;
    println!(
        "queued tasks:create  op={}  clientId injected={}",
        create.operation_id.as_str(),
        create.payload["clientId"]
    );
    let complete = outbox.enqueue(NewOutboxItem {
        operation_id: None,
        function_name: "tasks:complete".to_owned(),
        entity_kind: Some("task".to_owned()),
        entity_id: Some(create.operation_id.as_str().to_owned()),
        org_id: Some("org-demo".to_owned()),
        payload: json!({"orgId": "org-demo", "taskId": create.operation_id.as_str()}),
        created_at: 1_001,
    })?;
    println!(
        "queued tasks:complete op={}",
        complete.operation_id.as_str()
    );
    match outbox.enqueue(NewOutboxItem {
        operation_id: None,
        function_name: "tasks:addComment".to_owned(),
        entity_kind: None,
        entity_id: None,
        org_id: None,
        payload: json!({"body": "hello"}),
        created_at: 1_002,
    }) {
        Ok(_) => println!("unexpected: comment queued"),
        Err(error) => println!("gate refused tasks:addComment: {error}"),
    }

    let worker = OutboxWorker::new(
        Arc::clone(&store),
        Arc::new(server.clone()),
        OutboxWorkerConfig::default(),
    );

    println!("\n== offline: drain (lease -> fail -> backoff) ==");
    let report = worker.drain_once(10_000).await?;
    println!("{report:?}");
    let saved = outbox.get(&create.operation_id)?.expect("queued item");
    println!(
        "create is {:?}, attempt {}, next attempt at t={}ms",
        saved.state, saved.attempt_count, saved.next_attempt_at
    );

    println!("\n== reconnect with one lost acknowledgement ==");
    server.set_online(true);
    server.lose_next_acks(1);
    println!("{:?}", worker.drain_once(20_000).await?);
    println!("{:?}", worker.drain_once(60_000).await?);
    println!("server stats: {:?}", server.stats());

    println!("\n== materialize tasks:list ==");
    let materializer = RealtimeMaterializer::new(
        Arc::clone(&store),
        Arc::new(server.clone()),
        MaterializationSpec::new("tasks.open", "tasks:list", json!({"orgId": "org-demo"}))?,
        RealtimeMaterializerConfig {
            reconnect_delay_ms: 50,
        },
    );
    let (cancel, cancelled) = tokio::sync::watch::channel(false);
    let task = tokio::spawn(async move { materializer.run(cancelled).await });
    tokio::time::sleep(Duration::from_millis(100)).await;
    let _ = cancel.send(true);
    let run = task.await??;
    let snapshot = QuerySnapshotRepository::new(&store)
        .get("tasks.open")?
        .expect("snapshot");
    println!("{run:?}");
    println!(
        "snapshot revision {} -> {}",
        snapshot.revision,
        serde_json::to_string(&snapshot.result)?
    );
    Ok(())
}

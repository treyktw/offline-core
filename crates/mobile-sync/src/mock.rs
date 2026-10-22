//! In-memory reference backend used by tests, the demo, and the sample apps.
//!
//! It implements [`FunctionTransport`] and [`RealtimeTransport`] for the toy
//! catalog in `mobile-contracts` and enforces the same idempotency contracts the
//! client relies on, so offline -> reconnect -> drain -> materialize can be
//! exercised end to end without a network.
//!
//! Failure injection:
//! - [`InMemoryServer::set_online`] simulates losing and regaining the network.
//!   Going offline ends every open realtime stream, like a dropped socket.
//! - [`InMemoryServer::lose_next_acks`] applies the next N writes but reports a
//!   retryable transport error, which is the "server committed, response lost"
//!   case that makes idempotency keys necessary.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::{Arc, Mutex, MutexGuard};

use async_trait::async_trait;
use mobile_contracts::FunctionKind;
use serde_json::{Value, json};
use tokio::sync::watch;

use crate::{DeliveryError, FunctionTransport, QueryEventStream, RealtimeTransport};

/// Snapshot of server-side counters used by tests and demos.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServerStats {
    /// Write requests that reached the server.
    pub write_requests: u64,
    /// Writes that changed server state.
    pub writes_applied: u64,
    /// Writes recognized as replays of an already-applied identity.
    pub writes_deduplicated: u64,
    /// Realtime subscriptions accepted.
    pub subscriptions_opened: u64,
}

#[derive(Debug, Default)]
struct ServerState {
    online: bool,
    tasks: BTreeMap<String, Value>,
    comments: Vec<Value>,
    applied_entry_ids: HashSet<String>,
    applied_readings: HashSet<(String, i64)>,
    readings_by_device: HashMap<String, u64>,
    lost_acks_remaining: u32,
    stats: ServerStats,
}

/// Change notification shared with open realtime streams.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Broadcast {
    revision: u64,
    connection_epoch: u64,
    online: bool,
}

/// In-process backend with the toy task catalog.
///
/// Cloning is cheap and every clone talks to the same server state.
#[derive(Debug, Clone)]
pub struct InMemoryServer {
    inner: Arc<ServerInner>,
}

#[derive(Debug)]
struct ServerInner {
    state: Mutex<ServerState>,
    changes: watch::Sender<Broadcast>,
}

impl Default for InMemoryServer {
    fn default() -> Self {
        Self::new(true)
    }
}

impl InMemoryServer {
    /// Creates a server that starts online or offline.
    pub fn new(online: bool) -> Self {
        let (changes, _) = watch::channel(Broadcast {
            revision: 0,
            connection_epoch: 0,
            online,
        });
        Self {
            inner: Arc::new(ServerInner {
                state: Mutex::new(ServerState {
                    online,
                    ..ServerState::default()
                }),
                changes,
            }),
        }
    }

    /// Simulates network loss or recovery between the device and the server.
    pub fn set_online(&self, online: bool) {
        self.lock().online = online;
        self.inner.changes.send_modify(|broadcast| {
            if broadcast.online && !online {
                broadcast.connection_epoch += 1;
            }
            broadcast.online = online;
        });
    }

    /// Whether the simulated connection is up.
    pub fn is_online(&self) -> bool {
        self.lock().online
    }

    /// Applies the next `count` writes but reports a retryable error instead of
    /// the acknowledgement.
    pub fn lose_next_acks(&self, count: u32) {
        self.lock().lost_acks_remaining = count;
    }

    /// Returns server-side counters.
    pub fn stats(&self) -> ServerStats {
        self.lock().stats
    }

    /// Returns one task document.
    pub fn task(&self, task_id: &str) -> Option<Value> {
        self.lock().tasks.get(task_id).cloned()
    }

    /// Returns every task in id order.
    pub fn tasks(&self) -> Vec<Value> {
        self.lock().tasks.values().cloned().collect()
    }

    /// Returns accepted sensor readings for one device.
    pub fn reading_count(&self, device_id: &str) -> u64 {
        self.lock()
            .readings_by_device
            .get(device_id)
            .copied()
            .unwrap_or(0)
    }

    /// Writes directly on the server, as another client would.
    ///
    /// # Errors
    ///
    /// Returns [`DeliveryError`] when the function or arguments are invalid.
    pub fn apply_external(&self, name: &str, args: &Value) -> Result<Value, DeliveryError> {
        let mut state = self.lock();
        let result = apply_write(&mut state, name, args)?;
        drop(state);
        self.inner.publish();
        Ok(result.response)
    }

    fn lock(&self) -> MutexGuard<'_, ServerState> {
        self.inner.lock()
    }
}

impl ServerInner {
    fn lock(&self) -> MutexGuard<'_, ServerState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn publish(&self) {
        self.changes
            .send_modify(|broadcast| broadcast.revision += 1);
    }

    fn query(&self, name: &str, args: &Value) -> Result<Value, DeliveryError> {
        let state = self.lock();
        match name {
            "tasks:list" => {
                let org_id = args.get("orgId").and_then(Value::as_str);
                Ok(Value::Array(
                    state
                        .tasks
                        .values()
                        .filter(|task| {
                            org_id.is_none_or(|org| {
                                task.get("orgId").and_then(Value::as_str) == Some(org)
                            })
                        })
                        .cloned()
                        .collect(),
                ))
            }
            _ => Err(rejected("unknown_query", name)),
        }
    }
}

struct WriteOutcome {
    response: Value,
    changed: bool,
}

#[allow(clippy::too_many_lines)]
fn apply_write(
    state: &mut ServerState,
    name: &str,
    args: &Value,
) -> Result<WriteOutcome, DeliveryError> {
    let object = args
        .as_object()
        .ok_or_else(|| rejected("invalid_arguments", "arguments must be an object"))?;
    let text = |field: &str| {
        object
            .get(field)
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| rejected("invalid_arguments", field))
    };

    match name {
        // Server-enforced dedup: the client identity becomes the task id.
        "tasks:create" => {
            let client_id = text("clientId")?;
            if state.tasks.contains_key(client_id) {
                state.stats.writes_deduplicated += 1;
                return Ok(WriteOutcome {
                    response: json!({"taskId": client_id, "deduplicated": true}),
                    changed: false,
                });
            }
            state.tasks.insert(
                client_id.to_owned(),
                json!({
                    "_id": client_id,
                    "orgId": text("orgId")?,
                    "title": text("title")?,
                    "status": "open",
                    "loggedMinutes": 0,
                }),
            );
            Ok(WriteOutcome {
                response: json!({"taskId": client_id}),
                changed: true,
            })
        }
        // Intrinsic idempotency: assigning "done" twice is the same as once.
        "tasks:complete" => {
            let task_id = text("taskId")?;
            let task = state
                .tasks
                .get_mut(task_id)
                .ok_or_else(|| rejected("not_found", task_id))?;
            let changed = task["status"] != "done";
            task["status"] = json!("done");
            if !changed {
                state.stats.writes_deduplicated += 1;
            }
            Ok(WriteOutcome {
                response: json!({"taskId": task_id}),
                changed,
            })
        }
        // Per-item identity: each time entry is applied at most once.
        "tasks:logTime" => {
            let task_id = text("taskId")?.to_owned();
            if !state.tasks.contains_key(&task_id) {
                return Err(rejected("not_found", &task_id));
            }
            let entries = object
                .get("entries")
                .and_then(Value::as_array)
                .ok_or_else(|| rejected("invalid_arguments", "entries"))?;
            let mut added = 0;
            for entry in entries {
                let entry_id = entry
                    .get("entryId")
                    .and_then(Value::as_str)
                    .ok_or_else(|| rejected("invalid_arguments", "entryId"))?;
                let minutes = entry.get("minutes").and_then(Value::as_i64).unwrap_or(0);
                if state.applied_entry_ids.insert(entry_id.to_owned()) {
                    added += minutes;
                }
            }
            if added == 0 {
                state.stats.writes_deduplicated += 1;
            }
            let task = state.tasks.get_mut(&task_id).expect("task checked above");
            let total = task["loggedMinutes"].as_i64().unwrap_or(0) + added;
            task["loggedMinutes"] = json!(total);
            Ok(WriteOutcome {
                response: json!({"taskId": task_id, "loggedMinutes": total}),
                changed: added > 0,
            })
        }
        // Root identity plus per-item sequence.
        "sensors:ingestReadings" => {
            let device_id = text("deviceId")?.to_owned();
            let readings = object
                .get("readings")
                .and_then(Value::as_array)
                .ok_or_else(|| rejected("invalid_arguments", "readings"))?;
            let mut accepted = 0_u64;
            for reading in readings {
                let seq = reading
                    .get("seq")
                    .and_then(Value::as_i64)
                    .ok_or_else(|| rejected("invalid_arguments", "seq"))?;
                if state.applied_readings.insert((device_id.clone(), seq)) {
                    accepted += 1;
                }
            }
            *state.readings_by_device.entry(device_id).or_default() += accepted;
            if accepted == 0 {
                state.stats.writes_deduplicated += 1;
            }
            Ok(WriteOutcome {
                response: json!({"accepted": accepted}),
                changed: accepted > 0,
            })
        }
        // Not idempotent: replaying this would duplicate the comment. The
        // client gate keeps it out of the outbox, so it is online-only.
        "tasks:addComment" => {
            state.comments.push(args.clone());
            Ok(WriteOutcome {
                response: json!({"commentCount": state.comments.len()}),
                changed: true,
            })
        }
        _ => Err(rejected("unknown_function", name)),
    }
}

fn rejected(code: &str, message: &str) -> DeliveryError {
    DeliveryError {
        code: code.to_owned(),
        message: message.to_owned(),
        retryable: false,
    }
}

fn offline() -> DeliveryError {
    DeliveryError {
        code: "transport_offline".to_owned(),
        message: "server unreachable".to_owned(),
        retryable: true,
    }
}

#[async_trait]
impl FunctionTransport for InMemoryServer {
    async fn execute(
        &self,
        kind: FunctionKind,
        name: &str,
        args: Value,
    ) -> Result<Value, DeliveryError> {
        if kind == FunctionKind::Query {
            return Err(rejected(
                "query_not_queueable",
                "queries cannot execute from the durable outbox",
            ));
        }
        let mut state = self.lock();
        if !state.online {
            return Err(offline());
        }
        state.stats.write_requests += 1;
        let outcome = apply_write(&mut state, name, &args)?;
        if outcome.changed {
            state.stats.writes_applied += 1;
        }
        let lose_ack = state.lost_acks_remaining > 0;
        if lose_ack {
            state.lost_acks_remaining -= 1;
        }
        drop(state);
        if outcome.changed {
            self.inner.publish();
        }
        if lose_ack {
            return Err(DeliveryError {
                code: "transport_ack_lost".to_owned(),
                message: "connection dropped before the response arrived".to_owned(),
                retryable: true,
            });
        }
        Ok(outcome.response)
    }
}

#[async_trait]
impl RealtimeTransport for InMemoryServer {
    async fn subscribe_query(
        &self,
        name: &str,
        args: Value,
    ) -> Result<Box<dyn QueryEventStream>, DeliveryError> {
        // Subscribe before reading so a write landing between the two is
        // observed as a change rather than lost.
        let receiver = self.inner.changes.subscribe();
        let broadcast = *receiver.borrow();
        let initial = {
            let mut state = self.lock();
            if !state.online {
                return Err(offline());
            }
            state.stats.subscriptions_opened += 1;
            drop(state);
            self.inner.query(name, &args)?
        };
        Ok(Box::new(InMemoryQueryStream {
            server: Arc::clone(&self.inner),
            name: name.to_owned(),
            args,
            receiver,
            connection_epoch: broadcast.connection_epoch,
            pending_initial: Some(initial),
        }))
    }
}

struct InMemoryQueryStream {
    server: Arc<ServerInner>,
    name: String,
    args: Value,
    receiver: watch::Receiver<Broadcast>,
    connection_epoch: u64,
    pending_initial: Option<Value>,
}

#[async_trait]
impl QueryEventStream for InMemoryQueryStream {
    async fn next(&mut self) -> Option<Result<Value, DeliveryError>> {
        if let Some(initial) = self.pending_initial.take() {
            return Some(Ok(initial));
        }
        if self.receiver.changed().await.is_err() {
            return None;
        }
        let broadcast = *self.receiver.borrow_and_update();
        if !broadcast.online || broadcast.connection_epoch != self.connection_epoch {
            // The simulated socket dropped; the materializer will reconnect.
            return None;
        }
        Some(self.server.query(&self.name, &self.args))
    }
}

#[cfg(test)]
mod tests {
    use mobile_contracts::FunctionKind;
    use serde_json::json;

    use super::InMemoryServer;
    use crate::{FunctionTransport, RealtimeTransport};

    #[tokio::test]
    async fn offline_server_returns_retryable_errors() {
        let server = InMemoryServer::new(false);
        let error = server
            .execute(
                FunctionKind::Mutation,
                "tasks:create",
                json!({"orgId": "org-1", "clientId": "task-1", "title": "a"}),
            )
            .await
            .expect_err("offline server must refuse writes");
        assert!(error.retryable);
        assert!(
            server
                .subscribe_query("tasks:list", json!({}))
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn replayed_creates_are_deduplicated_by_client_identity() {
        let server = InMemoryServer::new(true);
        let args = json!({"orgId": "org-1", "clientId": "task-1", "title": "a"});
        for _ in 0..3 {
            server
                .execute(FunctionKind::Mutation, "tasks:create", args.clone())
                .await
                .expect("create should apply");
        }
        let stats = server.stats();
        assert_eq!(stats.writes_applied, 1);
        assert_eq!(stats.writes_deduplicated, 2);
        assert_eq!(server.tasks().len(), 1);
    }

    #[tokio::test]
    async fn streams_push_changes_and_end_when_the_link_drops() {
        let server = InMemoryServer::new(true);
        let mut stream = server
            .subscribe_query("tasks:list", json!({"orgId": "org-1"}))
            .await
            .expect("subscription should open");
        let initial = stream.next().await.expect("initial").expect("ok");
        assert_eq!(initial, json!([]));

        server
            .apply_external(
                "tasks:create",
                &json!({"orgId": "org-1", "clientId": "task-1", "title": "a"}),
            )
            .expect("external write should apply");
        let pushed = stream.next().await.expect("push").expect("ok");
        assert_eq!(pushed[0]["_id"], "task-1");

        server.set_online(false);
        assert!(
            stream.next().await.is_none(),
            "dropped link ends the stream"
        );
    }
}

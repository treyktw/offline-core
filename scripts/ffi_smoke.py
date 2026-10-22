"""Drives the compiled library through UniFFI-generated Python bindings.

Swift and Kotlin can't be compiled in a plain Linux CI box, but UniFFI's
Python backend reads the same metadata from the same cdylib, so this proves
the exported surface (records, enums, errors, objects, async methods) actually
works across the C ABI. Run it through `scripts/generate-bindings.sh --smoke`.
"""

import asyncio
import json
import sys

sys.path.insert(0, sys.argv[1])

import offline_core as core  # noqa: E402  (generated module)


async def main() -> None:
    contracts = {contract.name: contract for contract in core.function_contracts()}
    assert contracts["tasks:create"].offline_policy == core.OfflineDeliveryPolicy.QUEUEABLE
    assert (
        contracts["tasks:addComment"].offline_policy
        == core.OfflineDeliveryPolicy.REQUIRES_SERVER_IDEMPOTENCY
    )

    sdk = core.OfflineCore(
        core.SdkConfig(
            environment=core.Environment.DEVELOPMENT,
            installation_id="ffi-smoke",
            database_path=":memory:",
            database_key=bytes([0x5A] * 32),
        )
    )
    await sdk.start_realtime_query(
        core.RealtimeQueryRequest(
            subscription_key="tasks.open",
            function_name="tasks:list",
            args_json=json.dumps({"orgId": "org-demo"}),
        )
    )

    task_id = sdk.enqueue_create_task(
        core.CreateTaskIntent(org_id="org-demo", title="Inspect pump", created_at=1_000)
    )
    sdk.enqueue_complete_task(
        core.CompleteTaskIntent(org_id="org-demo", task_id=task_id, created_at=1_001)
    )
    try:
        sdk.enqueue_offline_function(
            core.OfflineFunctionIntent(
                function_name="tasks:addComment",
                operation_id=None,
                entity_kind=None,
                entity_id=None,
                org_id=None,
                payload_json="{}",
                created_at=1_002,
            )
        )
        raise AssertionError("gate should refuse non-idempotent writes")
    except core.SdkError.FunctionNotQueueable:
        pass

    offline = await sdk.synchronize_outbox_once(10_000)
    assert (offline.claimed, offline.retry_scheduled) == (2, 2), offline

    sdk.set_network_state(core.NetworkState.ONLINE)
    online = await sdk.synchronize_outbox_once(60_000)
    assert online.acknowledged == 2, online
    assert sdk.status().undelivered_operations == 0

    for _ in range(300):
        snapshot = sdk.tasks_snapshot("tasks.open")
        if snapshot and snapshot.tasks and snapshot.tasks[0].status == "done":
            break
        await asyncio.sleep(0.02)
    else:
        raise AssertionError("materialized snapshot never caught up")
    await sdk.stop_all_realtime()

    hos = core.compute_hos_projection(
        core.HosInput(
            events=[],
            config=core.HosConfig(
                cycle_rule=core.CycleRule.SIXTY_SEVEN, reminder_lead_seconds=1800
            ),
            as_of=1_735_689_600_000,
            recap_days=[],
        )
    )
    assert hos.snapshot.driving_remaining_ms == 11 * 3600 * 1000

    print(
        f"ffi smoke ok: task {snapshot.tasks[0].id} is {snapshot.tasks[0].status} "
        f"at revision {snapshot.revision}; {len(contracts)} contracts exported"
    )


asyncio.run(main())

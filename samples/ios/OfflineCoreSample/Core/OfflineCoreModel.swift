import Foundation
import Observation
import OfflineCore
import SwiftUI

/// Owns the Rust core for the lifetime of the app.
///
/// The sample uses the core's bundled in-memory backend, so the "Online"
/// toggle simulates connectivity instead of following NWPathMonitor. A real
/// app would forward NWPathMonitor updates to `setNetworkState` the same way.
@MainActor
@Observable
final class OfflineCoreModel {
    enum State: Equatable {
        case launching
        case ready
        case failed(String)
    }

    private static let orgId = "org-demo"
    private static let tasksKey = "tasks.open"

    private(set) var state: State = .launching
    private(set) var tasks: [TaskItem] = []
    private(set) var revision: Int64 = 0
    private(set) var undelivered: Int64 = 0
    private(set) var lastSync: String = "Not synchronized yet"
    var isOnline = false {
        didSet { applyNetworkState() }
    }

    private var core: OfflineCore?
    private var refreshTask: Task<Void, Never>?

    func start() async {
        guard core == nil else { return }
        do {
            let config = SdkConfig(
                environment: .development,
                installationId: InstallationIdentity.current(),
                databasePath: try databaseFileURL().path,
                databaseKey: try DatabaseKeyStore().loadOrCreate()
            )
            let core = try OfflineCore(config: config)
            self.core = core
            try await core.startRealtimeQuery(request: RealtimeQueryRequest(
                subscriptionKey: Self.tasksKey,
                functionName: "tasks:list",
                argsJson: #"{"orgId":"\#(Self.orgId)"}"#
            ))
            applyNetworkState()
            state = .ready
            startRefreshLoop()
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    func addTask(title: String) {
        guard let core, !title.trimmingCharacters(in: .whitespaces).isEmpty else { return }
        perform {
            _ = try core.enqueueCreateTask(intent: CreateTaskIntent(
                orgId: Self.orgId,
                title: title,
                createdAt: Self.now()
            ))
        }
    }

    func complete(_ task: TaskItem) {
        guard let core else { return }
        perform {
            _ = try core.enqueueCompleteTask(intent: CompleteTaskIntent(
                orgId: Self.orgId,
                taskId: task.id,
                createdAt: Self.now()
            ))
        }
    }

    func synchronize() async {
        guard let core else { return }
        do {
            let report = try await core.synchronizeOutboxOnce(now: Self.now())
            lastSync = "claimed \(report.claimed) · acked \(report.acknowledged) · retry \(report.retryScheduled)"
            refresh()
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    func updateScenePhase(_ phase: ScenePhase) {
        guard let core else { return }
        let value: ApplicationState = switch phase {
        case .active: .active
        case .inactive: .inactive
        case .background: .background
        @unknown default: .inactive
        }
        perform { try core.setApplicationState(value: value) }
    }

    private func applyNetworkState() {
        guard let core else { return }
        perform { try core.setNetworkState(value: isOnline ? .online : .offline) }
    }

    private func refresh() {
        guard let core else { return }
        perform {
            if let snapshot = try core.tasksSnapshot(subscriptionKey: Self.tasksKey) {
                tasks = snapshot.tasks
                revision = snapshot.revision
            }
            undelivered = try core.status().undeliveredOperations
        }
    }

    // Snapshots are durable rows, so polling them is cheap; a production app
    // would expose a revision callback instead.
    private func startRefreshLoop() {
        refreshTask?.cancel()
        refreshTask = Task { [weak self] in
            while !Task.isCancelled {
                self?.refresh()
                try? await Task.sleep(for: .milliseconds(500))
            }
        }
    }

    private func perform(_ body: () throws -> Void) {
        do {
            try body()
        } catch {
            state = .failed(error.localizedDescription)
        }
    }

    private static func now() -> Int64 {
        Int64(Date().timeIntervalSince1970 * 1000)
    }

    private func databaseFileURL() throws -> URL {
        let root = try FileManager.default.url(
            for: .applicationSupportDirectory,
            in: .userDomainMask,
            appropriateFor: nil,
            create: true
        )
        let directory = root.appending(path: "OfflineCore", directoryHint: .isDirectory)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        return directory.appending(path: "offline-core.sqlite")
    }
}

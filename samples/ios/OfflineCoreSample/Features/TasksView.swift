import OfflineCore
import SwiftUI

struct TasksView: View {
    let model: OfflineCoreModel
    @State private var draftTitle = ""

    var body: some View {
        NavigationStack {
            Group {
                switch model.state {
                case .launching:
                    ProgressView("Starting the Rust core…")
                case let .failed(message):
                    ContentUnavailableView(
                        "Offline core unavailable",
                        systemImage: "exclamationmark.triangle.fill",
                        description: Text(message)
                    )
                case .ready:
                    content
                }
            }
            .navigationTitle("Offline tasks")
        }
    }

    private var content: some View {
        List {
            Section {
                Toggle(isOn: Binding(get: { model.isOnline }, set: { model.isOnline = $0 })) {
                    Label(
                        model.isOnline ? "Online" : "Offline",
                        systemImage: model.isOnline ? "network" : "network.slash"
                    )
                }
                LabeledContent("Queued writes", value: "\(model.undelivered)")
                LabeledContent("Snapshot revision", value: "\(model.revision)")
                Button("Synchronize outbox") {
                    Task { await model.synchronize() }
                }
                Text(model.lastSync)
                    .font(.caption.monospaced())
                    .foregroundStyle(.secondary)
            }

            Section("New task") {
                HStack {
                    TextField("Title", text: $draftTitle)
                    Button("Queue") {
                        model.addTask(title: draftTitle)
                        draftTitle = ""
                    }
                }
            }

            Section("Server-confirmed tasks") {
                if model.tasks.isEmpty {
                    Text("Nothing materialized yet. Queue a task, go online, then synchronize.")
                        .foregroundStyle(.secondary)
                }
                ForEach(model.tasks, id: \.id) { task in
                    HStack {
                        Image(systemName: task.status == "done" ? "checkmark.circle.fill" : "circle")
                            .foregroundStyle(task.status == "done" ? .green : .secondary)
                        Text(task.title)
                        Spacer()
                        if task.status != "done" {
                            Button("Complete") { model.complete(task) }
                                .buttonStyle(.borderless)
                        }
                    }
                }
            }
        }
    }
}

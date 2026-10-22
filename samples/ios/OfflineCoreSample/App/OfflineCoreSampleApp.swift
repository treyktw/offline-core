import SwiftUI

@main
struct OfflineCoreSampleApp: App {
    @Environment(\.scenePhase) private var scenePhase
    @State private var model = OfflineCoreModel()

    var body: some Scene {
        WindowGroup {
            TasksView(model: model)
                .task {
                    await model.start()
                }
        }
        .onChange(of: scenePhase, initial: true) { _, phase in
            model.updateScenePhase(phase)
        }
    }
}

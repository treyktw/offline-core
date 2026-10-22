// swift-tools-version: 6.1
import PackageDescription

let package = Package(
    name: "OfflineCorePackage",
    platforms: [
        .iOS(.v17),
    ],
    products: [
        .library(name: "OfflineCore", targets: ["OfflineCore"]),
    ],
    targets: [
        .binaryTarget(
            name: "offline_coreFFI",
            path: "OfflineCore.xcframework"
        ),
        .target(
            name: "OfflineCore",
            dependencies: ["offline_coreFFI"]
        ),
    ]
)

# iOS sample

A tiny SwiftUI app on top of the shared Rust core. It uses the core's bundled
in-memory backend, so the Online toggle is a simulated network: queue tasks
while offline, flip it on, tap Synchronize, and watch the materialized list
catch up.

```bash
../../scripts/build-ios.sh     # builds target/ios/OfflineCorePackage
brew install xcodegen
xcodegen generate              # creates OfflineCoreSample.xcodeproj
open OfflineCoreSample.xcodeproj
```

Bundle ID: `com.example.offlinecore`. Change it before signing.

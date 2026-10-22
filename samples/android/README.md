# Android sample

A tiny Jetpack Compose app on top of the shared Rust core. It uses the core's
bundled in-memory backend, so the Online switch is a simulated network: queue
tasks while offline, flip it on, tap Synchronize, and watch the materialized
list catch up.

```bash
../../scripts/build-android.sh   # builds target/android/OfflineCore.aar
gradle wrapper                   # or open this folder in Android Studio
./gradlew :app:installDebug
```

Application ID: `com.example.offlinecore`.

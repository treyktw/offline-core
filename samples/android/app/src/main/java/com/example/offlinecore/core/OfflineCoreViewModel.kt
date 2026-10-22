package com.example.offlinecore.core

import android.app.Application
import androidx.lifecycle.AndroidViewModel
import androidx.lifecycle.viewModelScope
import java.io.File
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import uniffi.offline_core.ApplicationState
import uniffi.offline_core.CompleteTaskIntent
import uniffi.offline_core.CreateTaskIntent
import uniffi.offline_core.Environment
import uniffi.offline_core.NetworkState
import uniffi.offline_core.OfflineCore
import uniffi.offline_core.RealtimeQueryRequest
import uniffi.offline_core.SdkConfig
import uniffi.offline_core.TaskItem

data class OfflineCoreUiState(
    val phase: Phase = Phase.LAUNCHING,
    val message: String? = null,
    val tasks: List<TaskItem> = emptyList(),
    val revision: Long = 0,
    val undelivered: Long = 0,
    val lastSync: String = "Not synchronized yet",
    val isOnline: Boolean = false,
) {
    enum class Phase { LAUNCHING, READY, FAILED }
}

/**
 * Owns the Rust core. The sample runs against the core's bundled in-memory
 * backend, so the Online switch simulates connectivity; a real app would feed
 * ConnectivityManager callbacks into setNetworkState the same way.
 */
class OfflineCoreViewModel(application: Application) : AndroidViewModel(application) {
    private val mutableState = MutableStateFlow(OfflineCoreUiState())
    val state: StateFlow<OfflineCoreUiState> = mutableState.asStateFlow()

    private var core: OfflineCore? = null

    init {
        initialize()
    }

    fun setOnline(online: Boolean) {
        mutableState.update { it.copy(isOnline = online) }
        launchIo { core?.setNetworkState(if (online) NetworkState.ONLINE else NetworkState.OFFLINE) }
    }

    fun addTask(title: String) {
        if (title.isBlank()) return
        launchIo {
            requireNotNull(core).enqueueCreateTask(
                CreateTaskIntent(orgId = ORG_ID, title = title, createdAt = System.currentTimeMillis()),
            )
            refresh()
        }
    }

    fun complete(task: TaskItem) {
        launchIo {
            requireNotNull(core).enqueueCompleteTask(
                CompleteTaskIntent(orgId = ORG_ID, taskId = task.id, createdAt = System.currentTimeMillis()),
            )
            refresh()
        }
    }

    fun synchronize() {
        launchIo {
            val report = requireNotNull(core).synchronizeOutboxOnce(System.currentTimeMillis())
            mutableState.update {
                it.copy(lastSync = "claimed ${report.claimed} · acked ${report.acknowledged} · retry ${report.retryScheduled}")
            }
            refresh()
        }
    }

    fun setApplicationState(value: ApplicationState) {
        launchIo { core?.setApplicationState(value) }
    }

    private fun initialize() {
        viewModelScope.launch(Dispatchers.IO) {
            runCatching {
                val app = getApplication<Application>()
                OfflineCore(
                    SdkConfig(
                        environment = Environment.DEVELOPMENT,
                        installationId = InstallationIdentity.current(app),
                        databasePath = File(app.filesDir, "offline-core.sqlite").path,
                        databaseKey = DatabaseKeyStore(app).loadOrCreate(),
                    ),
                ).also { created ->
                    created.startRealtimeQuery(
                        RealtimeQueryRequest(
                            subscriptionKey = TASKS_KEY,
                            functionName = "tasks:list",
                            argsJson = """{"orgId":"$ORG_ID"}""",
                        ),
                    )
                    core = created
                }
            }.onSuccess {
                mutableState.update { it.copy(phase = OfflineCoreUiState.Phase.READY) }
                pollSnapshots()
            }.onFailure(::fail)
        }
    }

    // Snapshots are durable rows, so polling them is cheap; a production app
    // would expose a revision callback instead.
    private fun pollSnapshots() {
        viewModelScope.launch(Dispatchers.IO) {
            while (isActive) {
                runCatching { refresh() }.onFailure(::fail)
                delay(500)
            }
        }
    }

    private fun refresh() {
        val current = core ?: return
        val snapshot = current.tasksSnapshot(TASKS_KEY)
        val undelivered = current.status().undeliveredOperations
        mutableState.update {
            it.copy(
                tasks = snapshot?.tasks ?: it.tasks,
                revision = snapshot?.revision ?: it.revision,
                undelivered = undelivered,
            )
        }
    }

    private fun launchIo(block: suspend () -> Unit) {
        viewModelScope.launch(Dispatchers.IO) {
            runCatching { block() }.onFailure(::fail)
        }
    }

    private fun fail(error: Throwable) {
        mutableState.update {
            it.copy(phase = OfflineCoreUiState.Phase.FAILED, message = error.message ?: error::class.simpleName)
        }
    }

    override fun onCleared() {
        core?.close()
        core = null
        super.onCleared()
    }

    private companion object {
        const val ORG_ID = "org-demo"
        const val TASKS_KEY = "tasks.open"
    }
}

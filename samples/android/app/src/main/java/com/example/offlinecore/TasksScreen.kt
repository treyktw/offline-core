package com.example.offlinecore

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.safeDrawingPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.example.offlinecore.core.OfflineCoreUiState
import com.example.offlinecore.core.OfflineCoreViewModel

@Composable
fun TasksScreen(model: OfflineCoreViewModel) {
    val state by model.state.collectAsStateWithLifecycle()
    Surface(Modifier.fillMaxSize()) {
        when (state.phase) {
            OfflineCoreUiState.Phase.LAUNCHING -> Box(Modifier.fillMaxSize(), Alignment.Center) {
                CircularProgressIndicator()
            }
            OfflineCoreUiState.Phase.FAILED -> Box(Modifier.fillMaxSize().padding(32.dp), Alignment.Center) {
                Text(state.message ?: "Offline core unavailable")
            }
            OfflineCoreUiState.Phase.READY -> Ready(state, model)
        }
    }
}

@Composable
private fun Ready(state: OfflineCoreUiState, model: OfflineCoreViewModel) {
    var draftTitle by remember { mutableStateOf("") }
    LazyColumn(
        modifier = Modifier.fillMaxSize().safeDrawingPadding(),
        contentPadding = PaddingValues(20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            Text("Offline tasks", style = MaterialTheme.typography.headlineMedium, fontWeight = FontWeight.Bold)
        }
        item {
            Card(Modifier.fillMaxWidth()) {
                Column(Modifier.padding(16.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        Text(if (state.isOnline) "Online" else "Offline")
                        Spacer(Modifier.weight(1f))
                        Switch(checked = state.isOnline, onCheckedChange = model::setOnline)
                    }
                    Text("Queued writes: ${state.undelivered}")
                    Text("Snapshot revision: ${state.revision}")
                    Button(onClick = model::synchronize) { Text("Synchronize outbox") }
                    Text(state.lastSync, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall)
                }
            }
        }
        item {
            Row(verticalAlignment = Alignment.CenterVertically) {
                OutlinedTextField(
                    value = draftTitle,
                    onValueChange = { draftTitle = it },
                    label = { Text("New task") },
                    modifier = Modifier.weight(1f),
                )
                TextButton(onClick = {
                    model.addTask(draftTitle)
                    draftTitle = ""
                }) { Text("Queue") }
            }
        }
        if (state.tasks.isEmpty()) {
            item { Text("Nothing materialized yet. Queue a task, go online, then synchronize.") }
        }
        items(state.tasks, key = { it.id }) { task ->
            Card(Modifier.fillMaxWidth()) {
                Row(Modifier.padding(16.dp), verticalAlignment = Alignment.CenterVertically) {
                    Column(Modifier.weight(1f)) {
                        Text(task.title, fontWeight = FontWeight.SemiBold)
                        Text("${task.status} · ${task.loggedMinutes} min", style = MaterialTheme.typography.bodySmall)
                    }
                    if (task.status != "done") {
                        TextButton(onClick = { model.complete(task) }) { Text("Complete") }
                    }
                }
            }
        }
    }
}

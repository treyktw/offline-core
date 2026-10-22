package com.example.offlinecore

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import com.example.offlinecore.core.OfflineCoreViewModel
import uniffi.offline_core.ApplicationState

class MainActivity : ComponentActivity() {
    private val model: OfflineCoreViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        setContent {
            MaterialTheme(colorScheme = darkColorScheme()) {
                TasksScreen(model)
            }
        }
    }

    override fun onResume() {
        super.onResume()
        model.setApplicationState(ApplicationState.ACTIVE)
    }

    override fun onPause() {
        model.setApplicationState(ApplicationState.INACTIVE)
        super.onPause()
    }

    override fun onStop() {
        model.setApplicationState(ApplicationState.BACKGROUND)
        super.onStop()
    }
}

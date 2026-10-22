package com.example.offlinecore.core

import android.content.Context
import java.util.UUID

object InstallationIdentity {
    private const val PREFERENCES = "offline-core-installation"
    private const val KEY = "installation-id"

    fun current(context: Context): String {
        val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)
        return preferences.getString(KEY, null)?.takeIf(String::isNotBlank)
            ?: UUID.randomUUID().toString().lowercase().also {
                preferences.edit().putString(KEY, it).apply()
            }
    }
}

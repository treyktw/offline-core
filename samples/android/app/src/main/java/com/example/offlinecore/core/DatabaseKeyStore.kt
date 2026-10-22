package com.example.offlinecore.core

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import java.security.SecureRandom
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * Generates a random 32-byte database key once and stores it wrapped by a
 * non-exportable AES key in the Android Keystore. The Rust core only ever sees
 * the unwrapped bytes passed through [uniffi.offline_core.SdkConfig].
 */
class DatabaseKeyStore(private val context: Context) {
    private val preferences = context.getSharedPreferences("offline-core-database-key", Context.MODE_PRIVATE)

    fun loadOrCreate(): ByteArray {
        val encoded = preferences.getString(KEY_WRAPPED_DATABASE_KEY, null)
        return if (encoded == null) createAndStore() else decrypt(Base64.decode(encoded, Base64.NO_WRAP))
    }

    private fun createAndStore(): ByteArray {
        val databaseKey = ByteArray(32).also(SecureRandom()::nextBytes)
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, wrappingKey())
        val encrypted = cipher.doFinal(databaseKey)
        val payload = cipher.iv + encrypted
        preferences.edit()
            .putString(KEY_WRAPPED_DATABASE_KEY, Base64.encodeToString(payload, Base64.NO_WRAP))
            .commit()
        return databaseKey
    }

    private fun decrypt(payload: ByteArray): ByteArray {
        require(payload.size > IV_SIZE_BYTES) { "Invalid wrapped database key" }
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(
            Cipher.DECRYPT_MODE,
            wrappingKey(),
            GCMParameterSpec(128, payload.copyOfRange(0, IV_SIZE_BYTES)),
        )
        return cipher.doFinal(payload.copyOfRange(IV_SIZE_BYTES, payload.size)).also {
            require(it.size == 32) { "Invalid database key length" }
        }
    }

    private fun wrappingKey(): SecretKey {
        val keyStore = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (keyStore.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        return KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore").run {
            init(
                KeyGenParameterSpec.Builder(
                    KEY_ALIAS,
                    KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
                )
                    .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                    .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                    .setRandomizedEncryptionRequired(true)
                    .build(),
            )
            generateKey()
        }
    }

    private companion object {
        const val KEY_ALIAS = "offline-core-wrapping-key-v1"
        const val KEY_WRAPPED_DATABASE_KEY = "wrapped-database-key-v1"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
        const val IV_SIZE_BYTES = 12
    }
}

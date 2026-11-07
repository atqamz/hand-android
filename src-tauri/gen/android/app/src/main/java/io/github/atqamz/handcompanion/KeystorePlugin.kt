package io.github.atqamz.handcompanion

import android.app.Activity
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec
import org.json.JSONArray

@InvokeArg
class KeyBytes {
  var data: List<Int> = emptyList()
}

@TauriPlugin
class KeystorePlugin(activity: Activity) : Plugin(activity) {
  @Command
  @Synchronized
  fun seal(invoke: Invoke) {
    try {
      val plaintext = invoke.parseArgs(KeyBytes::class.java).data.toByteArray()
      val cipher = Cipher.getInstance(TRANSFORMATION)
      cipher.init(Cipher.ENCRYPT_MODE, key(create = true))
      invoke.resolve(response(cipher.iv + cipher.doFinal(plaintext)))
    } catch (error: Exception) {
      invoke.reject(error.message ?: "the Android Keystore could not encrypt the key", CODE_KEYSTORE)
    }
  }

  @Command
  @Synchronized
  fun open(invoke: Invoke) {
    val sealed = invoke.parseArgs(KeyBytes::class.java).data.toByteArray()
    if (sealed.size <= IV_BYTES) {
      invoke.reject("the stored key is not ciphertext", CODE_CORRUPT)
      return
    }
    val key = try {
      key(create = false)
    } catch (error: Exception) {
      null
    }
    if (key == null) {
      invoke.reject("the Android Keystore key that sealed the identity is gone", CODE_KEY_REPLACED)
      return
    }
    try {
      val cipher = Cipher.getInstance(TRANSFORMATION)
      cipher.init(
        Cipher.DECRYPT_MODE,
        key,
        GCMParameterSpec(TAG_BITS, sealed.copyOfRange(0, IV_BYTES))
      )
      invoke.resolve(response(cipher.doFinal(sealed.copyOfRange(IV_BYTES, sealed.size))))
    } catch (error: Exception) {
      invoke.reject(error.message ?: "the stored key is corrupt", CODE_CORRUPT)
    }
  }

  private fun key(create: Boolean): SecretKey? {
    val store = KeyStore.getInstance(KEYSTORE)
    store.load(null)
    val existing = try {
      store.getKey(ALIAS, null) as? SecretKey
    } catch (error: Exception) {
      null
    }
    if (existing != null || !create) {
      return existing
    }
    val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
    generator.init(
      KeyGenParameterSpec.Builder(
        ALIAS,
        KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT
      )
        .setKeySize(KEY_BITS)
        .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
        .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
        .setUserAuthenticationRequired(false)
        .build()
    )
    generator.generateKey()
    return store.getKey(ALIAS, null) as SecretKey
  }

  private fun response(bytes: ByteArray): JSObject {
    val values = JSONArray()
    bytes.forEach { values.put(it.toInt() and 0xff) }
    return JSObject().put("data", values)
  }

  private fun List<Int>.toByteArray(): ByteArray = ByteArray(size) { get(it).toByte() }

  private companion object {
    const val KEYSTORE = "AndroidKeyStore"
    const val ALIAS = "io.github.atqamz.handcompanion.iroh"
    const val TRANSFORMATION = "AES/GCM/NoPadding"
    const val KEY_BITS = 256
    const val TAG_BITS = 128
    const val IV_BYTES = 12
    const val CODE_KEY_REPLACED = "keyReplaced"
    const val CODE_CORRUPT = "corrupt"
    const val CODE_KEYSTORE = "keystore"
  }
}

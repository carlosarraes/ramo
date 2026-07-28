package io.github.carlosarraes.ramo.network

import android.content.Context
import android.util.Log

sealed interface BootstrapStatus {
    data object Ready : BootstrapStatus
    data object Failed : BootstrapStatus
}

object NativeNetworkBootstrap {
    private val libraryLoaded = runCatching { System.loadLibrary("ramo_mobile") }
        .onFailure { Log.e("Ramo", "Native library loading failed", it) }
        .isSuccess

    @Volatile
    var status: BootstrapStatus = BootstrapStatus.Failed
        private set

    @JvmStatic
    private external fun initializeNative(context: Context): Boolean

    @Synchronized
    fun initialize(context: Context): BootstrapStatus {
        if (!libraryLoaded) return BootstrapStatus.Failed
        status = try {
            if (initializeNative(context.applicationContext)) {
                BootstrapStatus.Ready
            } else {
                BootstrapStatus.Failed
            }
        } catch (error: Throwable) {
            Log.e("Ramo", "Native TLS initialization failed", error)
            BootstrapStatus.Failed
        }
        return status
    }
}

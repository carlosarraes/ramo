package io.github.carlosarraes.ramo

import android.app.Application
import io.github.carlosarraes.ramo.network.NativeNetworkBootstrap

class RamoApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        NativeNetworkBootstrap.initialize(this)
    }
}

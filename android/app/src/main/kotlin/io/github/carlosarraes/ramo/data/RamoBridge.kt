package io.github.carlosarraes.ramo.data

import io.github.carlosarraes.ramo.uniffi.coreVersion as nativeCoreVersion
import kotlinx.coroutines.CoroutineDispatcher
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext

interface RamoBridge {
    suspend fun coreVersion(): String
}

class NativeRamoBridge(
    private val dispatcher: CoroutineDispatcher = Dispatchers.IO,
) : RamoBridge {
    override suspend fun coreVersion(): String = withContext(dispatcher) {
        nativeCoreVersion()
    }
}

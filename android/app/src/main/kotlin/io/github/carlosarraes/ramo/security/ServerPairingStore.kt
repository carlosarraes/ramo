package io.github.carlosarraes.ramo.security

import android.content.Context
import org.json.JSONObject

data class ServerPairing(
    val endpoint: String,
    val clientId: String,
    val token: String,
    val pairedAt: Long,
) {
    override fun toString(): String = "ServerPairing(endpoint=$endpoint, clientId=$clientId, pairedAt=$pairedAt)"
}

interface PairingStore {
    fun read(): ServerPairing?
    fun write(pairing: ServerPairing)
    fun clear()
}

class ServerPairingStore(context: Context) : PairingStore {
    private val blobs = EncryptedBlobStore(context, "ramo.review-map.aes.v1", "review-map-pairing.enc")

    override fun read(): ServerPairing? = blobs.read()?.let { bytes ->
        val json = JSONObject(bytes.toString(Charsets.UTF_8))
        ServerPairing(
            endpoint = json.getString("endpoint"),
            clientId = json.getString("clientId"),
            token = json.getString("token"),
            pairedAt = json.getLong("pairedAt"),
        )
    }

    override fun write(pairing: ServerPairing) {
        val json = JSONObject()
            .put("endpoint", pairing.endpoint)
            .put("clientId", pairing.clientId)
            .put("token", pairing.token)
            .put("pairedAt", pairing.pairedAt)
        blobs.write(json.toString().toByteArray(Charsets.UTF_8))
    }

    override fun clear() = blobs.clear()

    internal fun storedBytesForTest(): ByteArray? = blobs.storedBytesForTest()
}

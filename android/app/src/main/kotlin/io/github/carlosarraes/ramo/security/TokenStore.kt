package io.github.carlosarraes.ramo.security

import android.content.Context

interface TokenStore {
    fun read(): String?
    fun write(token: String)
    fun clear()
}

class SecureTokenStore(context: Context) : TokenStore {
    private val blobs = EncryptedBlobStore(context, "ramo.mobile.aes.v1", "github-token.enc")

    override fun read(): String? = blobs.read()?.toString(Charsets.UTF_8)
    override fun write(token: String) = blobs.write(token.toByteArray(Charsets.UTF_8))
    override fun clear() = blobs.clear()
}

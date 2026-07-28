package io.github.carlosarraes.ramo

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.lifecycle.viewmodel.compose.viewModel
import io.github.carlosarraes.ramo.auth.AuthState
import io.github.carlosarraes.ramo.auth.AuthViewModel
import io.github.carlosarraes.ramo.auth.NativeAuthenticator
import io.github.carlosarraes.ramo.auth.TokenScreen
import io.github.carlosarraes.ramo.security.SecureTokenStore
import io.github.carlosarraes.ramo.ui.theme.RamoTheme

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent {
            RamoTheme {
                val auth: AuthViewModel = viewModel {
                    AuthViewModel(SecureTokenStore(applicationContext), NativeAuthenticator())
                }
                val state by auth.state.collectAsState()
                LaunchedEffect(Unit) { auth.restore() }
                when (val current = state) {
                    is AuthState.SignedIn -> Box(Modifier.fillMaxSize(), contentAlignment = Alignment.Center) {
                        Button(onClick = auth::signOut) { Text("Signed in as ${current.login} · Sign out") }
                    }
                    else -> TokenScreen(current, auth::validate)
                }
            }
        }
    }
}

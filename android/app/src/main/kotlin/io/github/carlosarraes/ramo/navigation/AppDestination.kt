package io.github.carlosarraes.ramo.navigation

sealed interface AppDestination {
    data object Inbox : AppDestination
    data class Review(val repository: String, val number: Long) : AppDestination
    data object Settings : AppDestination
}

package io.github.carlosarraes.ramo.navigation

sealed interface AppDestination {
    data object Inbox : AppDestination
    data class ReviewMap(val repository: String, val number: Long) : AppDestination
    data class ReviewFile(val repository: String, val number: Long, val path: String) : AppDestination
    data object Settings : AppDestination
}

package io.github.carlosarraes.ramo.review

import android.content.Context

class ReviewPreferencesStore(context: Context) {
    private val preferences = context.getSharedPreferences("review-preferences", Context.MODE_PRIVATE)
    var codeSize: Int
        get() = preferences.getInt("code-size", 13).coerceIn(11, 20)
        set(value) { preferences.edit().putInt("code-size", value.coerceIn(11, 20)).apply() }
}

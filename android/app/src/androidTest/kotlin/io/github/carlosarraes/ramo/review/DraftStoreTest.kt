package io.github.carlosarraes.ramo.review

import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class DraftStoreTest {
    @Test fun encryptedDraftSurvivesReloadWithoutPlaintextAtRest() {
        val context = ApplicationProvider.getApplicationContext<android.content.Context>()
        val store = SecureDraftStore(context)
        store.clearAll()
        val comment = DraftCommentUi(
            "id", "ramo/ramo", 7, "sha", "src/lib.rs", CommentSideUi.Right, 42, 42,
            emptyList(), listOf("line"), emptyList(), "plaintext review secret",
        )
        store.save(DraftReviewUi("ramo/ramo", 7, "sha", "overall", listOf(comment)))
        val bytes = context.filesDir.listFiles { file -> file.name.startsWith("review-") }!!.single().readBytes()
        assertFalse(bytes.toString(Charsets.UTF_8).contains("plaintext review secret"))
        assertEquals("plaintext review secret", store.load("ramo/ramo", 7)!!.comments.single().body)
        store.clearAll()
    }
}

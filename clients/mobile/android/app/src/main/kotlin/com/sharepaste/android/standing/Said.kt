package com.sharepaste.android.standing

import androidx.annotation.StringRes

/**
 * What a Standing Action has to say about what it just did.
 *
 * Two parts, because the in-app bands have two: the outcome named in a word or
 * so, then the sentence. `OFFERED`, `RECALLED`, `MAY BE STALE` and `NOT PAIRED`
 * are the same labels `NoticeBanner` puts above the same sentences — **a
 * Standing Action and a press on the History are one operation**, and reporting
 * it in two idioms would make them look like two. That symmetry is the only
 * reason this type exists; a Toast could carry the sentence alone and did.
 *
 * The label is unresolved and the sentence is resolved, which is not an
 * oversight: every arm that builds one of these already has a resolved sentence
 * in hand (often with an argument in it), and none of them wants a second
 * `getString` at the call site for a constant.
 *
 * **Neither half may ever contain an Entry's text.** Nothing here has the
 * plaintext to leak — `RecallAttempt` deliberately carries none — and that has
 * to stay true of anything added.
 */
internal data class Said(@param:StringRes val label: Int, val sentence: String)

package com.sharepaste.android

import java.io.BufferedReader

/**
 * This process's own log, as a test can read it.
 *
 * The only way to observe a Standing Action from outside it. The transparent
 * activity has no composition, returns nothing to its caller and finishes — its
 * whole output is a Toast and a log line carrying the same sentence, produced by
 * the same call. A Toast is drawn by the system in a window this app cannot
 * inspect, so the log line is what an assertion can hold, and it is exactly as
 * good a witness: if it is there, `report` ran and the Toast went with it.
 *
 * `logd` hands an app without `READ_LOGS` only the entries its own uid wrote,
 * which is precisely the scope wanted here — instrumentation runs in the app's
 * process, so the Standing Action's lines are ours.
 */
object Logcat {

    /**
     * Wait for a line under [tag] that satisfies [predicate], and return it.
     *
     * Polls rather than streams: the activity being waited on is in the middle
     * of starting, and a `logcat -d` snapshot taken now and again is simpler
     * than a reader thread that has to be torn down when the test fails.
     */
    fun await(
        tag: String,
        what: String,
        timeoutSeconds: Long = 30,
        predicate: (String) -> Boolean,
    ): String {
        val deadline = System.nanoTime() + timeoutSeconds * 1_000_000_000L
        var seen = emptyList<String>()
        while (System.nanoTime() < deadline) {
            seen = lines(tag)
            seen.firstOrNull(predicate)?.let { return it }
            Thread.sleep(200)
        }
        throw AssertionError(
            "$what: no line under \"$tag\" matched in ${timeoutSeconds}s. What was logged:\n" +
                seen.joinToString("\n").ifEmpty { "(nothing at all)" },
        )
    }

    /** Every line this process has logged under [tag] since [clear]. */
    fun lines(tag: String): List<String> {
        val process = ProcessBuilder("logcat", "-d", "-s", tag)
            .redirectErrorStream(true)
            .start()
        return try {
            process.inputStream.bufferedReader().use(BufferedReader::readLines)
                .filter { it.contains(tag) }
        } finally {
            process.destroy()
        }
    }

    /**
     * Drop everything logged so far.
     *
     * Load-bearing between the halves of a force-stop sequence: the run before
     * this one logged the same sentences, and a test that matched one of those
     * would pass without the app having done anything at all.
     */
    fun clear() {
        ProcessBuilder("logcat", "-c").start().waitFor()
    }
}

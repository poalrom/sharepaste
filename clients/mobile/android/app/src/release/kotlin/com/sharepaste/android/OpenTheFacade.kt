package com.sharepaste.android

/**
 * How the shipped app opens its facade.
 *
 * **This file is the whole of the release transport policy, and it has no
 * branch.** The value handed to `requireHttps` is one expression that reads
 * nothing: not a file, not a system property, not a field of [app], not a
 * `BuildConfig` flag other than the one the build file sets. There is no code
 * path in a release artifact that can relax it, which is a stronger guarantee
 * than a runtime flag guarded by `BuildConfig.DEBUG` — that flag is a branch
 * someone can be persuaded to reach, and this is an absence.
 *
 * The debug source set carries its own copy with one extra line, for the one
 * caller a test cannot construct: ticket 12's Standing Actions run in a process
 * with no instrumentation attached, so they necessarily use
 * `SharepasteApplication.repository` and cannot be handed a facade of their own
 * the way every other instrumented test is. See `app/src/debug` for the whole of
 * that concession, and `ShippedTransportPolicyTest`, which reads this file's
 * text and fails if a branch ever appears in it.
 *
 * [app] is here because a facade needs a database path and a `Context` to derive
 * one from. It takes no part in the policy.
 */
internal fun openTheFacade(app: SharepasteApplication): SharepasteRepository =
    SharepasteRepository.open(app, requireHttps = BuildConfig.REQUIRE_HTTPS)

//! The **History**: a user's entries, newest first, and the acts this device
//! owes the relay for them.
//!
//! Two SQLite tables behind one interface, because they are one order (ADR
//! 0014). `entries_cache` holds the Entries and `pending_uploads` the acts, and
//! the History is this device's pending acts — in the order it will send them —
//! above the entries the relay has ordered by Last Use. Nothing outside this
//! module knows either table exists, which is what lets the seam between the two
//! regions be a fact of one query rather than an agreement between four files
//! coupled through SQLite.
//!
//! One thing does cross, and it is named rather than hidden: [`Cursor`] carries
//! this module's encoding of the order as two opaque numbers, because a foreign
//! binding mirrors the record field by field and cannot be handed something it
//! cannot name. Nothing in the core reads them — a cursor comes off a row and
//! goes back into [`page`] unopened — but the field names are mirrored in
//! `clients/mobile/ffi/src/types.rs` and the desktop's `commands.rs`, so
//! *renaming* them is a cross-tier edit. What each number means is still this
//! module's alone.
//!
//! **The queue rowid does not leave.** It was an act's identity handle and its
//! sort key at once, and it is honest as neither: moving an act to the back
//! deletes and re-inserts it (ADR 0016), so a number a caller held would address
//! a row that no longer exists. Callers name an [`Act`] — which carries the
//! handle and will not show it — or the Entry an act belongs to.
//!
//! **There is no cap on the queue.** An act this device has not delivered is
//! undelivered clipboard content, and this queue used to evict the oldest of
//! them silently to keep a number under a thousand (ADR 0014). What bounds it is
//! the relay coming back.

use crate::errors::AppError;
use crate::event::{CoreEvent, Queued};
use crate::platform::EventSink;
use data_encoding::HEXLOWER;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};

/// How many entries the relay has ordered one user may keep here.
///
/// **The one owner of the number, and the only one.** A shell draws a list-end
/// sentinel where this bites — "the oldest of a hundred" — so it has to name the
/// same hundred this prunes at, and a shell holding its own copy announces a
/// limit that is no longer the one taking rows away. Re-exported as
/// [`facade::MAX_PER_USER`](crate::facade::MAX_PER_USER); the desktop's
/// `store/history.ts` carries a copy it cannot import across an IPC boundary and
/// pins it against this declaration in `store.test.ts`, which fails if this line
/// moves.
pub const MAX_PER_USER: i64 = 100;

/// How long one of them is kept, measured from its Last Use.
pub(crate) const MAX_AGE_MS: i64 = 30 * 24 * 60 * 60 * 1000;

/// The first region the caps apply to.
///
/// The regions themselves are written into [`HISTORY`], one per arm, because
/// each arm also states the fact that region is ordered by and the two belong
/// together. Nothing outside this file names the number.
const RANK_SETTLED: i64 = 2;

/// The most rows one page may ask for.
///
/// Deliberately not [`MAX_PER_USER`]: that bounds the region the relay has
/// ordered, and the un-flushed region is unbounded on purpose — a hundred and
/// fifty offline captures are a hundred and fifty rows, and evicting one to
/// protect a number is the trade ADR 0014 refuses. A ceiling still exists so a
/// caller cannot ask for the whole table by accident.
pub(crate) const MAX_PAGE: i64 = 1_000;

const KIND_CAPTURE: &str = "capture";
const KIND_USE: &str = "use";

// -- what the History is made of -------------------------------------------

/// One row of the History as this device holds it.
#[derive(Debug, Clone, PartialEq)]
pub struct CachedEntry {
    pub user_id: String,
    /// This device's own name for the row, assigned at insert and stable until
    /// the row is deleted.
    ///
    /// An Entry exists from the moment of capture, on the device that made it,
    /// and the relay's id is what makes it *shared* rather than what makes it
    /// real (ADR 0016). So the row needs an identity the relay did not assign,
    /// and this is it: nothing about a flush changes it, which is what lets a
    /// shell keep a row's selection and keyboard cursor across one.
    pub local_id: i64,
    /// The relay's id, once the relay has taken the act that created this row.
    ///
    /// `None` is an Entry this device holds and no other device knows of yet.
    pub relay_id: Option<i64>,
    pub plaintext: Option<String>,
    pub created_at: i64,
    /// The moment of this entry's most recent **Use**, on whichever device.
    ///
    /// The only fact the settled region is ordered by and the only fact either
    /// retention cap measures. Equal to `created_at` for an entry never used
    /// since capture, which is the truth about it rather than a placeholder.
    pub last_use: i64,
    pub device_id: String,
    /// What this row still owes the relay.
    ///
    /// Decoded from the region the order put the row in, here and only here, so
    /// a caller has no business knowing which region is the refused one.
    pub queued: Queued,
    /// Where a page resuming after this row begins.
    pub resume_from: Cursor,
}

/// Where a page of the History resumes from.
///
/// Three parts — which region a row is in, its place inside that region, and the
/// row's own id to keep the order total when two rows share a place — because
/// reading back the same three the list is ordered by is what makes crossing the
/// seam between the regions no different from any other page boundary.
///
/// **Take one off a row rather than assembling one.** Two of the three parts are
/// this module's encoding of the queue's order, and nothing in the core reads
/// them: a cursor comes off [`CachedEntry::resume_from`] and goes back into
/// [`page`] unopened. They are `pub` only because a foreign binding mirrors this
/// record field by field, and a mirror cannot be built from parts it cannot
/// name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct Cursor {
    /// Which region: refused, pending, settled.
    pub rank: i64,
    /// The place inside that region.
    pub ord: i64,
    /// This device's own id for the row.
    pub id: i64,
}

/// What one act asks the relay to do.
///
/// One queue and not two, because the order between the two kinds is the point:
/// pendings reach the relay in the order they were made, so an outage cannot
/// reorder what happened during it. Two queues draining independently would let
/// a capture made after a use land before it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ActKind {
    /// A **Capture**, encrypted on the way into the queue.
    Capture(Vec<u8>),
    /// A **Use** of an Entry the relay already holds, named by the relay's id.
    /// No ciphertext: the entry is unchanged, and only when it was last used has
    /// moved.
    Use(i64),
}

/// One act this device owes the relay.
///
/// `id` is the module's own handle and cannot be read: it is the queue rowid,
/// which is the act's *sort key*, and a sort key is not a name — moving an act
/// to the back deletes and re-inserts it (ADR 0016). Everything a caller does to
/// an act it does by handing this value back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Act {
    id: i64,
    pub(crate) kind: ActKind,
    /// The Entry this act belongs to: the row a capture created, or the row a
    /// use acts on.
    ///
    /// What lets a queued act be found from its entry, and what the flush
    /// reconciles a relay id onto. `None` only on a row a database written
    /// before this column existed left behind.
    pub(crate) entry_id: Option<i64>,
    /// When the act was made. Reset when it moves to the back of the queue,
    /// because that is a fresh act rather than a retry.
    pub(crate) captured_at: i64,
    pub(crate) attempts: i64,
    pub(crate) last_error: Option<String>,
}

/// One Entry as the relay delivered it — a backfill row, or an SSE frame.
#[derive(Debug, Clone)]
pub(crate) struct RelayEntry<'a> {
    pub(crate) user_id: &'a str,
    pub(crate) relay_id: i64,
    pub(crate) ciphertext: &'a [u8],
    pub(crate) plaintext: Option<&'a str>,
    pub(crate) created_at: i64,
    pub(crate) last_use: i64,
    pub(crate) device_id: &'a str,
}

// -- what a change to it amounts to ----------------------------------------

/// What one change to the History did to it.
///
/// A description of what happened rather than a row count, and the only thing
/// [`announce`] reads: every mutating operation here answers one, so the mapping
/// from "what happened" to "what a shell is told" is one table in one place. It
/// used to be five copies of `PendingCount` — each preceded by its own
/// queue-depth read — and seven of `HistoryChanged`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Change {
    /// The queue depth now, and `None` when the queue is not what changed.
    ///
    /// **A move is not a depth change.** Re-copying, recalling or resending
    /// something still queued sends its act to the back: the act moved, it did
    /// not multiply, and a count that ticked would report a change that did not
    /// happen.
    pub(crate) depth: Option<i64>,
    /// Whether the rows changed places.
    ///
    /// **A flush is not a reorder.** The relay stamps a pending act on arrival
    /// exactly where this device already showed it, so a row changes region
    /// without changing place — which is why settling an act costs a shell one
    /// row to redraw and never a refetch of a hundred.
    pub(crate) reordered: bool,
}

impl Change {
    /// A change to the order alone: nothing joined the queue and nothing left it.
    pub(crate) fn reorder(reordered: bool) -> Change {
        Change { depth: None, reordered }
    }
}

/// The one place a change to the History becomes events for a shell.
///
/// | what changed | what a shell is told |
/// |---|---|
/// | the rows changed places | `HistoryChanged` |
/// | the queue is a different depth | `PendingCount` |
/// | neither | nothing |
///
/// In that order, and it matters: a shell that refetches on `HistoryChanged`
/// wants the depth it draws beside the list to be the one that goes with the
/// rows it has just asked for.
///
/// Events about *one row* — `EntryAdded`, `EntryDeleted`, `EntrySettled`,
/// `EntryRefused` — are deliberately not here. Each has exactly one site, each
/// carries a payload only that site can build, and each is emitted before this
/// one: the row's own news first, then what it did to the list.
pub(crate) fn announce(events: &dyn EventSink, user_id: &str, change: &Change) {
    if change.reordered {
        events.emit(CoreEvent::HistoryChanged { user_id: user_id.to_string() });
    }
    if let Some(count) = change.depth {
        events.emit(CoreEvent::PendingCount { user_id: user_id.to_string(), count });
    }
}

/// What queueing a **Capture** produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Captured {
    /// This device's id for the Entry the capture created — the handle
    /// everything above `storage/` uses for it, and the one the act carries.
    pub(crate) local_id: i64,
    pub(crate) change: Change,
}

/// What storing one relay-delivered Entry did.
///
/// `local_id` is the row it landed on, which the caller needs because it is the
/// only id that crosses the facade: the relay's id is what this path arrived
/// with, and the shell has never heard of it.
///
/// Two facts beside it and not one, because three paths reach the same relay id
/// and each owes the shell a different event. `first_insert` says an Entry is new
/// here, and exactly one path may raise `EntryAdded` for it. The [`Change`] says
/// whether an Entry already here moved, which is what `HistoryChanged` reports.
///
/// The pair is what tells a **Use** apart from the relay's echo of an Entry this
/// device uploaded: both are repeat ingests of a row the cache holds, and only
/// the use carries a later Last Use. Deriving "it moved" from `!first_insert`
/// would make every one of this device's own uploads announce a reorder that did
/// not happen, and cost both shells a full refetch each time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stored {
    pub local_id: i64,
    pub first_insert: bool,
    pub(crate) change: Change,
}

/// What this device already holds that matches text just copied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Held {
    /// An act for this text is still queued, against this Entry. Re-copying it
    /// is the same act as copying it (ADR 0012), and [`resend`] carries that.
    Queued(i64),
    /// The relay has this text under an Entry this device holds: a **Use** of it.
    Entry(i64),
    /// Nothing here matches, so this is a capture.
    Nothing,
}

/// What the relay said when it took one act.
pub(crate) enum Taken {
    /// A **Capture** the relay has now named and stamped.
    Capture { relay_id: i64, created_at: i64, last_use: i64 },
    /// A **Use** the relay recorded against an Entry it holds.
    Use { relay_id: i64, last_use: i64 },
    /// A queued **Use** naming an Entry the relay no longer has.
    UseVanished { relay_id: i64 },
}

/// What the relay stamped on the row an act belonged to.
///
/// `None` in a field means the relay said nothing about that number, not that
/// nobody knows it: a use does not restamp a creation, and a use of an entry the
/// relay has since dropped stamps neither yet still takes the act out of the
/// queue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Stamp {
    pub(crate) created_at: Option<i64>,
    pub(crate) last_use: Option<i64>,
}

/// What settling one act did — everything the caller has to act on, in one
/// value rather than a tuple threaded out of a lock scope.
pub(crate) struct Settlement {
    /// The relay's id for an Entry it took and nobody wants any more.
    ///
    /// The withdrawal race, and the only evidence of it there is: the upload
    /// awaited with the database lock released, and a delete inside that window
    /// took the queued act with it. Its caller has to take the Entry back off
    /// the relay — reconciling would attach a relay id to a row somebody
    /// deleted, or re-create it.
    pub(crate) withdrawn: Option<i64>,
    /// What the relay stamped on the row that has stopped waiting. `None` for a
    /// withdrawn act: its row is gone, and `EntryDeleted` already said so.
    pub(crate) stamp: Option<Stamp>,
    pub(crate) change: Change,
}

/// What refusing one act did.
pub(crate) struct Refusal {
    /// Whether *this* caller claimed the refusal.
    ///
    /// A session being replaced leaves the outgoing uploader mid-flush while the
    /// incoming one starts, so two of them can read one head and both be told
    /// 413. One act earns one refusal, and `refused_at` is what says which of the
    /// two claimed it.
    pub(crate) claimed: bool,
    pub(crate) change: Change,
}

// -- reading the History ----------------------------------------------------

/// One page of the History: this device's pending acts, in the order it will
/// send them, above the entries the relay has ordered by Last Use (ADR 0014).
///
/// `before` resumes after the last row of the previous page. Keyset paging over
/// the cursor the list is ordered by is what makes crossing the seam between the
/// regions no different from any other page boundary: the page after the last
/// pending row is the first settled one.
///
/// The seam does not move at a flush. The relay stamps a pending act on arrival
/// exactly where this device already showed it, so a row changes region without
/// changing place.
pub fn page(
    conn: &Connection,
    user_id: &str,
    before: Option<Cursor>,
    limit: i64,
) -> Result<Vec<CachedEntry>, AppError> {
    let limit = limit.clamp(1, MAX_PAGE);
    // `?2` is the presence of a cursor, so one statement serves both pages
    // rather than two copies of a fifty-line union drifting apart.
    let (has_cursor, rank, ord, id) = match before {
        Some(c) => (1, c.rank, c.ord, c.id),
        None => (0, 0, 0, 0),
    };
    let mut stmt = conn.prepare(HISTORY)?;
    let mut rows: Vec<CachedEntry> = stmt
        .query_map(params![user_id, has_cursor, rank, ord, id, limit], map_row)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.shrink_to_fit();
    Ok(rows)
}

/// The whole plaintext of one entry.
///
/// `None` covers both "no such entry" and "this device cannot decrypt it":
/// neither has a plaintext to hand over.
pub fn plaintext_of(
    conn: &Connection,
    user_id: &str,
    entry_id: i64,
) -> Result<Option<String>, AppError> {
    let pt: Option<Option<String>> = conn
        .query_row(
            "SELECT plaintext FROM entries_cache WHERE user_id = ?1 AND local_id = ?2",
            params![user_id, entry_id],
            |r| r.get::<_, Option<String>>(0),
        )
        .optional()?;
    Ok(pt.flatten())
}

/// How many acts this device still owes the relay for one Pairing.
pub fn depth(conn: &Connection, user_id: &str) -> Result<i64, AppError> {
    let n: i64 = conn.query_row(
        "SELECT COUNT(*) FROM pending_uploads WHERE user_id = ?1",
        params![user_id],
        |r| r.get(0),
    )?;
    Ok(n)
}

/// The relay's id for one entry, if the relay has named it.
///
/// Three answers and not two, because "there is no such row" and "the relay has
/// never named it" want opposite treatment: one is a mistake, the other is the
/// withdraw ADR 0016 exists for. Flattening them made `delete_entry` answer
/// `Ok(())` to a row that never existed, on a user with no pairing at all — so
/// the third answer is [`AppError::NotFound`], naming the row.
pub(crate) fn relay_id_of(
    conn: &Connection,
    user_id: &str,
    entry_id: i64,
) -> Result<Option<i64>, AppError> {
    conn.query_row(
        "SELECT relay_id FROM entries_cache WHERE user_id = ?1 AND local_id = ?2",
        params![user_id, entry_id],
        |r| r.get::<_, Option<i64>>(0),
    )
    .optional()?
    .ok_or_else(|| AppError::NotFound(format!("no entry {entry_id} on this device")))
}

/// What this device already holds that matches text just copied.
///
/// The device makes this judgement because the relay cannot: `crypto::encrypt`
/// draws a fresh nonce every call, so the same plaintext never produces the same
/// ciphertext, and a hash over the text **exactly as copied** is what stands in
/// (ADR 0012).
///
/// **The queue first, and only then the entries.** An un-flushed capture is both
/// a row and an act, and only the act is actionable: the entry has no relay id to
/// record a use against, and re-copying it is the same act as copying it. A
/// settled entry misses in the queue — its act is long gone — and falls through.
///
/// The queue is searched head-first, so a repeat copy moves the *oldest* matching
/// pending rather than leaving it stranded behind its own duplicate. An
/// **Undecryptable** row has no plaintext and therefore no hash, so it can never
/// match: it must not swallow a copy of text it cannot prove it holds.
pub(crate) fn recognise(conn: &Connection, user_id: &str, text: &str) -> Result<Held, AppError> {
    let hash = plaintext_sha256(text);
    let queued: Option<i64> = conn
        .query_row(
            "SELECT local_entry_id FROM pending_uploads
              WHERE user_id = ?1 AND kind = ?2 AND plaintext_sha256 = ?3
                AND local_entry_id IS NOT NULL
              ORDER BY rowid ASC LIMIT 1",
            params![user_id, KIND_CAPTURE, hash],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(entry_id) = queued {
        return Ok(Held::Queued(entry_id));
    }
    // Ordered by Last Use so that, in the impossible-in-practice case of two
    // rows holding the same text, the one already at the head is the one used.
    let held: Option<i64> = conn
        .query_row(
            "SELECT local_id FROM entries_cache
              WHERE user_id = ?1 AND plaintext_sha256 = ?2
              ORDER BY last_use DESC, local_id DESC LIMIT 1",
            params![user_id, hash],
            |r| r.get(0),
        )
        .optional()?;
    Ok(match held {
        Some(entry_id) => Held::Entry(entry_id),
        None => Held::Nothing,
    })
}

// -- the acts owed ----------------------------------------------------------

/// The next act the relay should be asked to take.
///
/// **Refused acts are skipped.** A refusal is a fact about *that act* and will be
/// refused identically forever, so leaving it at the head would block everything
/// behind it on something waiting cannot fix (ADR 0015). Skipping it is not a
/// loosening of the order: what would have followed it was never going to be
/// delivered by queueing behind it.
pub(crate) fn next_act(conn: &Connection, user_id: &str) -> Result<Option<Act>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rowid, kind, entry_id, ciphertext, captured_at, attempts, last_error,
                local_entry_id
           FROM pending_uploads
          WHERE user_id = ?1 AND refused_at IS NULL
          ORDER BY rowid ASC LIMIT 1",
    )?;
    let row = stmt.query_row(params![user_id], map_act).optional()?;
    Ok(row)
}

/// Every act this device owes the relay, in the order it will send them.
///
/// *What is queued, in order* — the question nothing could answer before, so a
/// caller that wanted to know what sat at the head decrypted the queue's own
/// ciphertext to find out. The text is not here because it is not the act's:
/// every act names the Entry it belongs to (ADR 0016), and [`plaintext_of`] is
/// what reads that.
///
/// Refused acts are in their place rather than missing. They are still owed;
/// they are simply not deliverable, which is what [`next_act`] steps over them
/// for.
///
/// Test-only, for the reason [`open_in_memory`](super::open_in_memory) is:
/// production drains the queue an act at a time and never needs the whole of
/// it. The callers are the suites that used to have to guess.
#[cfg(test)]
pub(crate) fn owed(conn: &Connection, user_id: &str) -> Result<Vec<Act>, AppError> {
    let mut stmt = conn.prepare(
        "SELECT rowid, kind, entry_id, ciphertext, captured_at, attempts, last_error,
                local_entry_id
           FROM pending_uploads
          WHERE user_id = ?1
          ORDER BY rowid ASC",
    )?;
    let acts = stmt
        .query_map(params![user_id], map_act)?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(acts)
}

// -- capture and use --------------------------------------------------------

/// Queue a **Capture**: the Entry it makes and the act that owes it to the relay.
///
/// **One transaction, and the only door to either half.** An Entry exists from
/// the moment of capture (ADR 0016), and the row is what the queued act is *for*:
/// a queue row with no entry is an act nobody can see or withdraw, and an entry
/// with no queued act is content that never reaches the relay. Either half alone
/// is a worse state than neither. Three separate test helpers used to
/// re-implement this pair, so a change to the production ordering would have left
/// two of the three suites green.
///
/// **`created_at` and `last_use` are zero**, and stay zero until the relay stamps
/// the act. There is one clock in this system and it is the relay's (ADR 0014): a
/// provisional device timestamp would order this row against relay-stamped ones
/// by a number no other device agrees with, and would then be silently
/// overwritten. Zero is the same "not stamped yet" marker `last_use` already
/// carries before its backfill runs. What orders the row meanwhile is its place
/// in the queue.
///
/// **It does not prune**, and must not: the caps bound what the relay has
/// ordered, and an act still owed to the relay is not competing for that room.
/// Evicting one would destroy clipboard content that has reached nowhere else.
///
/// The hash of the plaintext is stored on both halves so [`recognise`] can match
/// a repeat copy against either. The queue holds ciphertext, and ciphertext
/// cannot be compared: `crypto::encrypt` draws a fresh nonce every call.
pub(crate) fn capture(
    conn: &Connection,
    user_id: &str,
    ciphertext: &[u8],
    plaintext: &str,
    device_id: &str,
    at: i64,
) -> Result<Captured, AppError> {
    let hash = plaintext_sha256(plaintext);
    let tx = conn.unchecked_transaction()?;
    tx.execute(
        "INSERT INTO entries_cache
            (user_id, relay_id, ciphertext, plaintext, plaintext_sha256, created_at, last_use, device_id)
         VALUES (?1, NULL, ?2, ?3, ?4, 0, 0, ?5)",
        params![user_id, ciphertext, plaintext, hash, device_id],
    )?;
    let local_id = tx.last_insert_rowid();
    tx.execute(
        "INSERT INTO pending_uploads
            (user_id, kind, local_entry_id, ciphertext, plaintext_sha256, captured_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![user_id, KIND_CAPTURE, local_id, ciphertext, hash, at],
    )?;
    let depth = depth(&tx, user_id)?;
    tx.commit()?;
    Ok(Captured { local_id, change: Change { depth: Some(depth), reordered: false } })
}

/// Queue a **Use** of an Entry, because the relay could not be reached.
///
/// Carries both ids the act needs: `relay_id` is what the relay is told about,
/// `entry_id` is the row this device shows and orders.
///
/// The relay stamps it on arrival, exactly as it already stamps a pending
/// capture: one clock in the system, and the flush order is what preserves what
/// actually happened during the outage.
pub(crate) fn queue_use(
    conn: &Connection,
    user_id: &str,
    entry_id: i64,
    relay_id: i64,
    at: i64,
) -> Result<Change, AppError> {
    conn.execute(
        "INSERT INTO pending_uploads (user_id, kind, entry_id, local_entry_id, captured_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![user_id, KIND_USE, relay_id, entry_id, at],
    )?;
    Ok(Change { depth: Some(depth(conn, user_id)?), reordered: false })
}

/// Record a **Use** the relay confirmed, without touching anything else about
/// the entry.
///
/// Keyed on the relay's id, because every caller is holding the answer to a relay
/// call it just made.
///
/// Deliberately does not prune. A use only ever *raises* one entry's Last Use, so
/// the set of prunable rows can only shrink; pruning stays on insert, where
/// something new has arrived to make room for.
pub(crate) fn record_use(
    conn: &Connection,
    user_id: &str,
    relay_id: i64,
    last_use: i64,
) -> Result<Change, AppError> {
    let n = conn.execute(
        "UPDATE entries_cache SET last_use = ?3 WHERE user_id = ?1 AND relay_id = ?2",
        params![user_id, relay_id, last_use],
    )?;
    Ok(Change::reorder(n > 0))
}

/// Move the act queued against one entry to the back of the queue, as of `at`.
///
/// **Resend, re-copy and recall are one operation here**, because they are one
/// act. A pending has no relay id, so there is nothing to record a use against;
/// the queue's order is the only record that this text is the most recent thing
/// this device did, and back of the queue is the head of the History (ADR 0014).
/// No new act and no third kind of act: the act moves rather than multiplying.
///
/// **A fresh act and not a retry.** The attempt count, the last error and the
/// refusal are left behind with the row this replaces, which is what makes a
/// **Resend** carry nothing forward from the refusal that preceded it (ADR 0015).
///
/// The last act by queue position is the one that moves: an entry can hold
/// several — a capture and the uses re-copied onto it — and the row already sorts
/// by the newest of them.
///
/// `None` when nothing is queued against the entry. The row is then beyond
/// anything this can do for it, which is the state a withdrawn act leaves and
/// nothing else reaches.
pub(crate) fn resend(
    conn: &Connection,
    user_id: &str,
    entry_id: i64,
    at: i64,
) -> Result<Option<Change>, AppError> {
    let tx = conn.unchecked_transaction()?;
    let rowid: Option<i64> = tx.query_row(
        "SELECT MAX(rowid) FROM pending_uploads WHERE user_id = ?1 AND local_entry_id = ?2",
        params![user_id, entry_id],
        |r| r.get(0),
    )?;
    let Some(rowid) = rowid else {
        return Ok(None);
    };
    let moved = tx.execute(
        "INSERT INTO pending_uploads
            (user_id, kind, entry_id, local_entry_id, ciphertext, plaintext_sha256, captured_at)
         SELECT user_id, kind, entry_id, local_entry_id, ciphertext, plaintext_sha256, ?2
           FROM pending_uploads WHERE rowid = ?1",
        params![rowid, at],
    )?;
    if moved == 1 {
        tx.execute("DELETE FROM pending_uploads WHERE rowid = ?1", params![rowid])?;
    }
    tx.commit()?;
    Ok(Some(Change::reorder(true)))
}

// -- the flush --------------------------------------------------------------

/// Take a settled act off the queue and record what the relay said about it.
///
/// **The withdrawal race, decided here.** The upload awaited with the database
/// lock released, and a delete inside that window took the queued act with it.
/// A delete that removes no row is that, and the only evidence of it there is:
/// the answer carries [`Settlement::withdrawn`] instead of a stamp, because
/// reconciling would attach a relay id to a row somebody deleted, or re-create
/// it.
///
/// **Reconciliation, not insertion.** The Entry exists from the moment of capture
/// (ADR 0016), so the flush has nothing to create and nothing to announce: it
/// hands the row the relay's id, the `created_at` every other device will see it
/// dated by, and its Last Use. The `local_id` does not move, which is what lets
/// both shells keep a row's selection and keyboard cursor across a flush.
///
/// Off the queue *first*, and then the relay's word: the prune inside the
/// reconciliation exempts a row with an act still owed, so attaching first would
/// leave the row exempt for the write meant to bring the cache back inside its
/// cap.
///
/// A failed reconciliation is logged and swallowed: the act is on the relay,
/// which is the part that matters. It is **not** repaired by the next backfill,
/// and the log says so rather than promising otherwise — the relay-delivered path
/// is keyed on `relay_id`, so it matches nothing and inserts a second row for the
/// same text, leaving the first stranded un-named at the bottom of the list.
pub(crate) fn settle(
    conn: &Connection,
    user_id: &str,
    act: &Act,
    taken: Taken,
    now_ms: i64,
) -> Result<Settlement, AppError> {
    let acked =
        conn.execute("DELETE FROM pending_uploads WHERE rowid = ?1", params![act.id])? > 0;
    let mut withdrawn = None;
    let mut stamp = None;
    let mut reordered = false;
    match taken {
        Taken::Capture { relay_id, created_at, last_use } if acked => {
            stamp = Some(Stamp { created_at: Some(created_at), last_use: Some(last_use) });
            attach(conn, user_id, act.entry_id, relay_id, created_at, last_use, now_ms);
        }
        Taken::Capture { relay_id, .. } => withdrawn = Some(relay_id),
        Taken::Use { relay_id, last_use } => {
            stamp = Some(Stamp { created_at: None, last_use: Some(last_use) });
            reordered = record_use(conn, user_id, relay_id, last_use)?.reordered;
        }
        Taken::UseVanished { relay_id } => {
            stamp = Some(Stamp { created_at: None, last_use: None });
            tracing::info!(
                entry_id = relay_id,
                "dropped a queued use of an entry the relay no longer has"
            );
        }
    }
    Ok(Settlement {
        withdrawn,
        stamp,
        change: Change { depth: Some(depth(conn, user_id)?), reordered },
    })
}

/// Mark one act **Refused**: the relay turned it down for what it is.
///
/// It stays in the queue and stops being deliverable, which is the whole shape of
/// a refusal (ADR 0015). The alternative this replaced deleted the row and wrote
/// a warning to the log — code destroying a person's clipboard content and
/// telling only the log about it. The depth is therefore unchanged, and is
/// reported anyway: a shell that draws the queue draws the refusal beside it.
///
/// [`Refusal::claimed`] is false when the act was already refused, and its caller
/// has to know for the same reason the withdrawal race matters.
pub(crate) fn refuse(
    conn: &Connection,
    user_id: &str,
    act: &Act,
    at: i64,
    reason: &str,
) -> Result<Refusal, AppError> {
    let claimed = conn.execute(
        "UPDATE pending_uploads SET refused_at = ?2, last_error = ?3
          WHERE rowid = ?1 AND refused_at IS NULL",
        params![act.id, at, reason],
    )? > 0;
    Ok(Refusal { claimed, change: Change { depth: Some(depth(conn, user_id)?), reordered: false } })
}

/// Count one failed attempt against an act, and keep what the relay said.
///
/// The act stays exactly where it is: being out of reach is never a refusal, and
/// surviving a relay that is not there is the entire purpose of the queue.
pub(crate) fn record_failure(
    conn: &Connection,
    act: &Act,
    err: &str,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE pending_uploads SET attempts = attempts + 1, last_error = ?2 WHERE rowid = ?1",
        params![act.id, err],
    )?;
    Ok(())
}

// -- what the relay delivers ------------------------------------------------

/// Store one Entry the relay has named, then bring the user's History back inside
/// its caps.
///
/// Keyed on `relay_id`, because that is the only handle its callers have: this is
/// the relay-delivered path — a backfill row, an SSE frame — and what they carry
/// is the relay's number for the entry. The row's `local_id` is allocated on the
/// insert and never touched again.
///
/// The answer comes from the database rather than from a caller's bookkeeping:
/// `ON CONFLICT DO UPDATE` reports one changed row either way and SQLite has no
/// "was it an insert" flag, so both facts are read before the write.
///
/// Only a plaintext this device can read has a hash, and only a hash makes a
/// repeat copy recognisable — so the hash is derived here, from the one thing it
/// is ever derived from, rather than passed in beside the text it is a hash of.
///
/// Both caps measure from Last Use, so ciphertext in regular use is never aged
/// out. Eviction and ordering reading different facts is the version where the
/// count cap deletes the row sitting at the top of the list.
pub(crate) fn store(
    conn: &Connection,
    e: RelayEntry<'_>,
    now_ms: i64,
) -> Result<Stored, AppError> {
    let hash = e.plaintext.map(plaintext_sha256);
    let tx = conn.unchecked_transaction()?;
    let held: Option<i64> = tx
        .query_row(
            "SELECT last_use FROM entries_cache WHERE user_id = ?1 AND relay_id = ?2",
            params![e.user_id, e.relay_id],
            |r| r.get(0),
        )
        .optional()?;
    tx.execute(
        "INSERT INTO entries_cache
            (user_id, relay_id, ciphertext, plaintext, plaintext_sha256, created_at, last_use, device_id)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
         ON CONFLICT (user_id, relay_id) DO UPDATE SET
            ciphertext       = excluded.ciphertext,
            plaintext        = COALESCE(excluded.plaintext, entries_cache.plaintext),
            plaintext_sha256 = COALESCE(excluded.plaintext_sha256, entries_cache.plaintext_sha256),
            created_at       = excluded.created_at,
            last_use         = excluded.last_use,
            device_id        = excluded.device_id",
        params![
            e.user_id, e.relay_id, e.ciphertext, e.plaintext, hash,
            e.created_at, e.last_use, e.device_id
        ],
    )?;
    let local_id = tx.query_row(
        "SELECT local_id FROM entries_cache WHERE user_id = ?1 AND relay_id = ?2",
        params![e.user_id, e.relay_id],
        |r| r.get(0),
    )?;
    prune(&tx, e.user_id, now_ms)?;
    tx.commit()?;
    Ok(Stored {
        local_id,
        first_insert: held.is_none(),
        change: Change::reorder(held.is_some_and(|was| was != e.last_use)),
    })
}

/// Clear a cached plaintext once its entry stops decrypting.
///
/// Necessary because [`store`] COALESCEs a NULL incoming plaintext onto the
/// stored one, which is right for a ciphertext-only backfill and wrong for a
/// decryption failure. `sync::decryptor::ingest` calls this on its
/// `undecryptable` branch so [`plaintext_of`] stops handing back plaintext the
/// app has just told the user it cannot decrypt.
///
/// The hash goes with it, and must: a row this device can no longer read must not
/// go on matching copies of text it can no longer prove it holds.
pub(crate) fn mark_undecryptable(
    conn: &Connection,
    user_id: &str,
    relay_id: i64,
) -> Result<(), AppError> {
    conn.execute(
        "UPDATE entries_cache SET plaintext = NULL, plaintext_sha256 = NULL
          WHERE user_id = ?1 AND relay_id = ?2",
        params![user_id, relay_id],
    )?;
    Ok(())
}

// -- forgetting -------------------------------------------------------------

/// Forget one entry, and withdraw the acts queued against it.
///
/// **The acts go with a row the relay has never named.** No other device knows of
/// it, so there is nothing out there to take back — and the queue is durable
/// across a force-quit, so without this there is no way to stop a mistaken copy
/// reaching the relay when it comes back (ADR 0016). A row the relay *has* named
/// has already been deleted there by the caller that got here.
///
/// The row and the acts go together, and in that order for no reason that
/// matters: nothing else can observe the gap.
pub(crate) fn forget_entry(
    conn: &Connection,
    user_id: &str,
    entry_id: i64,
) -> Result<Change, AppError> {
    let tx = conn.unchecked_transaction()?;
    let named: Option<Option<i64>> = tx
        .query_row(
            "SELECT relay_id FROM entries_cache WHERE user_id = ?1 AND local_id = ?2",
            params![user_id, entry_id],
            |r| r.get(0),
        )
        .optional()?;
    let withdraw = matches!(named, Some(None));
    tx.execute(
        "DELETE FROM entries_cache WHERE user_id = ?1 AND local_id = ?2",
        params![user_id, entry_id],
    )?;
    let mut depth_now = None;
    if withdraw {
        tx.execute(
            "DELETE FROM pending_uploads WHERE user_id = ?1 AND local_entry_id = ?2",
            params![user_id, entry_id],
        )?;
        depth_now = Some(depth(&tx, user_id)?);
    }
    tx.commit()?;
    Ok(Change { depth: depth_now, reordered: false })
}

/// Forget the Entry the relay has just deleted, and answer this device's id for
/// it.
///
/// The frame names the relay's id and the shells know the row by its own, so the
/// translation has to happen before the row is gone.
pub(crate) fn forget_relay_entry(
    conn: &Connection,
    user_id: &str,
    relay_id: i64,
) -> Result<Option<i64>, AppError> {
    let local_id: Option<i64> = conn
        .query_row(
            "SELECT local_id FROM entries_cache WHERE user_id = ?1 AND relay_id = ?2",
            params![user_id, relay_id],
            |r| r.get(0),
        )
        .optional()?;
    if let Some(local_id) = local_id {
        conn.execute(
            "DELETE FROM entries_cache WHERE user_id = ?1 AND local_id = ?2",
            params![user_id, local_id],
        )?;
    }
    Ok(local_id)
}

/// Empty one Pairing's History: every Entry, and every act owed for one.
///
/// The queue goes with the entries and has to: left standing it would repopulate
/// exactly what was just cleared on the next flush, which is what it used to do
/// (ADR 0016).
pub(crate) fn forget_all(conn: &Connection, user_id: &str) -> Result<Change, AppError> {
    let tx = conn.unchecked_transaction()?;
    tx.execute("DELETE FROM entries_cache WHERE user_id = ?1", params![user_id])?;
    tx.execute("DELETE FROM pending_uploads WHERE user_id = ?1", params![user_id])?;
    let depth_now = depth(&tx, user_id)?;
    tx.commit()?;
    Ok(Change { depth: Some(depth_now), reordered: true })
}

/// Forget every Entry for one Pairing, leaving the queue where it is.
///
/// The half `forget_pairing` needs, and deliberately not [`forget_all`]: that one
/// is `clear_history`'s, where the Pairing survives the wipe and a queue left
/// standing would put back exactly what was cleared. Here the Pairing itself is
/// going away.
pub(crate) fn forget_entries(conn: &Connection, user_id: &str) -> Result<(), AppError> {
    conn.execute("DELETE FROM entries_cache WHERE user_id = ?1", params![user_id])?;
    Ok(())
}

// -- inside -----------------------------------------------------------------

/// What goes in `plaintext_sha256`, defined once by the column's owner.
///
/// Over the text **exactly as copied** — no trimming, no normalisation — so the
/// same URL with and without a trailing newline is two entries. Trimming would
/// recognise more and would make a **Recall** hand back the *stored* variant
/// rather than the one just copied, which in a shell is the difference between a
/// command that runs and one that waits. See ADR 0012.
///
/// Reachable outside the module for one caller: `migrations` hashes the
/// plaintexts an installed cache already holds, on the upgrade that adds the
/// column.
pub(crate) fn plaintext_sha256(text: &str) -> String {
    let mut h = Sha256::new();
    h.update(text.as_bytes());
    HEXLOWER.encode(&h.finalize())
}

/// Attach what the relay recorded to the row a capture already created.
///
/// An update and never an insert, so the row keeps the `local_id` both shells are
/// keying on. Prunes, because the row has just joined the region the caps bound.
fn attach(
    conn: &Connection,
    user_id: &str,
    entry_id: Option<i64>,
    relay_id: i64,
    created_at: i64,
    last_use: i64,
    now_ms: i64,
) {
    let Some(local_entry_id) = entry_id else {
        tracing::warn!(
            relay_id,
            "a queued capture named no local entry; the relay's echo will insert it"
        );
        return;
    };
    match attach_relay_id(conn, user_id, local_entry_id, relay_id, created_at, last_use, now_ms) {
        Ok(0) => tracing::info!(
            local_entry_id,
            relay_id,
            "the entry this act created is gone; nothing to reconcile"
        ),
        Ok(_) => {}
        Err(e) => tracing::warn!(
            err = %e, local_entry_id, relay_id,
            "could not attach the relay's id; its echo will arrive as a second row"
        ),
    }
}

fn attach_relay_id(
    conn: &Connection,
    user_id: &str,
    local_id: i64,
    relay_id: i64,
    created_at: i64,
    last_use: i64,
    now_ms: i64,
) -> Result<usize, AppError> {
    let tx = conn.unchecked_transaction()?;
    let n = tx.execute(
        "UPDATE entries_cache
            SET relay_id = ?3, created_at = ?4, last_use = ?5
          WHERE user_id = ?1 AND local_id = ?2 AND relay_id IS NULL",
        params![user_id, local_id, relay_id, created_at, last_use],
    )?;
    prune(&tx, user_id, now_ms)?;
    tx.commit()?;
    Ok(n)
}

/// Bring the rows the relay has ordered back inside both caps.
///
/// **Every row with an act still owed is exempt**, which is the settled region
/// and it alone competing for the hundred (ADR 0014). Two shapes qualify: a
/// capture the relay has not named, and an entry it *has* named that carries a
/// queued **Use**. The second is the one a `relay_id IS NULL` test would miss, and
/// missing it is worse than it looks: a three-week-old entry re-copied offline is
/// at the top of the list with the lowest `last_use` in the cache, so a full cache
/// would evict exactly the row the person just reached for and strand its act in
/// the queue with nothing to reconcile onto.
///
/// Both caps measure from `last_use`, and a row the relay has not stamped has none
/// — it would age out on the write that stored it. More to the point, the caps
/// exist to bound a *cache* of what the relay holds; an act still owed is
/// undelivered content, and discarding it to protect a display invariant is the
/// trade ADR 0014 refuses.
///
/// `local_id` is the tiebreak rather than the relay's id, and has to be: it is the
/// only number every row has. The migration copies an installed cache in relay-id
/// order precisely so the two agree about relative age for everything that
/// predates it.
fn prune(conn: &Connection, user_id: &str, now_ms: i64) -> Result<(), AppError> {
    conn.execute(
        "DELETE FROM entries_cache
          WHERE user_id = ?1
            AND local_id IN (
              SELECT e.local_id FROM entries_cache e
               WHERE e.user_id = ?1
                 AND e.relay_id IS NOT NULL
                 AND NOT EXISTS (
                       SELECT 1 FROM pending_uploads p
                        WHERE p.user_id = e.user_id AND p.local_entry_id = e.local_id)
            )
            AND (
              last_use < (?2 - ?3)
              OR local_id NOT IN (
                SELECT e.local_id FROM entries_cache e
                 WHERE e.user_id = ?1
                   AND e.relay_id IS NOT NULL
                   AND NOT EXISTS (
                         SELECT 1 FROM pending_uploads p
                          WHERE p.user_id = e.user_id AND p.local_entry_id = e.local_id)
                 ORDER BY e.last_use DESC, e.local_id DESC
                 LIMIT ?4
              )
            )",
        params![user_id, now_ms, MAX_AGE_MS, MAX_PER_USER],
    )?;
    Ok(())
}

/// The three regions of the History as one ordered list.
///
/// The order is `(rank ASC, ord DESC, local_id DESC)` and every arm supplies its
/// own `ord`, which is why this is a `UNION ALL` rather than a join with a
/// `CASE`: the three groups are ordered by three different facts, and each arm
/// says which one it is ordered by beside the rank that selects it.
///
/// `MAX(rowid)` for rank 1 and not the first: two offline re-copies of one entry
/// enqueue two acts, and the row has to rise on the second. The bare `last_error`
/// beside `MAX(rowid)` in rank 0 takes its value from the row that produced the
/// maximum — SQLite's documented behaviour for a single `min`/`max` aggregate —
/// which is the reason the refusal reason is readable here at all without a second
/// correlated lookup.
const HISTORY: &str = "
WITH history AS (
  SELECT e.user_id, e.local_id, e.relay_id, e.plaintext,
         e.created_at, e.last_use, e.device_id,
         0 AS rank, MAX(p.rowid) AS ord, p.last_error AS refused_reason
    FROM entries_cache e
    JOIN pending_uploads p
      ON p.user_id = e.user_id AND p.local_entry_id = e.local_id
         AND p.refused_at IS NOT NULL
   WHERE e.user_id = ?1
   GROUP BY e.local_id

  UNION ALL

  SELECT e.user_id, e.local_id, e.relay_id, e.plaintext,
         e.created_at, e.last_use, e.device_id,
         1 AS rank, MAX(p.rowid) AS ord, NULL AS refused_reason
    FROM entries_cache e
    JOIN pending_uploads p
      ON p.user_id = e.user_id AND p.local_entry_id = e.local_id
         AND p.refused_at IS NULL
   WHERE e.user_id = ?1
     AND NOT EXISTS (
           SELECT 1 FROM pending_uploads r
            WHERE r.user_id = e.user_id AND r.local_entry_id = e.local_id
              AND r.refused_at IS NOT NULL)
   GROUP BY e.local_id

  UNION ALL

  SELECT e.user_id, e.local_id, e.relay_id, e.plaintext,
         e.created_at, e.last_use, e.device_id,
         2 AS rank, e.last_use AS ord, NULL AS refused_reason
    FROM entries_cache e
   WHERE e.user_id = ?1
     AND NOT EXISTS (
           SELECT 1 FROM pending_uploads p
            WHERE p.user_id = e.user_id AND p.local_entry_id = e.local_id)
)
SELECT user_id, local_id, relay_id, plaintext, created_at, last_use,
       device_id, rank, ord, refused_reason
  FROM history
 WHERE ?2 = 0
    OR rank > ?3
    OR (rank = ?3 AND (ord < ?4 OR (ord = ?4 AND local_id < ?5)))
 ORDER BY rank ASC, ord DESC, local_id DESC
 LIMIT ?6
";

fn map_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<CachedEntry> {
    let local_id: i64 = r.get(1)?;
    let rank: i64 = r.get(7)?;
    let ord: i64 = r.get(8)?;
    let refused_reason: Option<String> = r.get(9)?;
    Ok(CachedEntry {
        user_id: r.get(0)?,
        local_id,
        relay_id: r.get(2)?,
        plaintext: r.get(3)?,
        created_at: r.get(4)?,
        last_use: r.get(5)?,
        device_id: r.get(6)?,
        queued: match refused_reason {
            Some(reason) => Queued::Refused(reason),
            None if rank < RANK_SETTLED => Queued::Pending,
            None => Queued::Settled,
        },
        resume_from: Cursor { rank, ord, id: local_id },
    })
}

fn map_act(r: &rusqlite::Row<'_>) -> rusqlite::Result<Act> {
    let kind: String = r.get(1)?;
    Ok(Act {
        id: r.get(0)?,
        kind: match kind.as_str() {
            KIND_USE => ActKind::Use(r.get(2)?),
            // Anything else is a capture. The discriminator is written by this
            // module alone, and a row the migration produced is one.
            _ => ActKind::Capture(r.get(3)?),
        },
        entry_id: r.get(7)?,
        captured_at: r.get(4)?,
        attempts: r.get(5)?,
        last_error: r.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::open_in_memory;
    use crate::testing::RecordingSink;

    /// Ingest one relay-delivered entry. `id` is the relay's, which is the only
    /// id this path knows; the answer is the row's own.
    fn ins(c: &Connection, user: &str, id: i64, pt: Option<&str>, ts: i64, now: i64) -> i64 {
        used(c, user, id, pt, ts, ts, now)
    }

    fn used(
        c: &Connection, user: &str, id: i64, pt: Option<&str>, ts: i64, last_use: i64, now: i64,
    ) -> i64 {
        store(c, RelayEntry {
            user_id: user, relay_id: id, ciphertext: b"ct", plaintext: pt,
            created_at: ts, last_use, device_id: "d1",
        }, now).unwrap().local_id
    }

    /// A **Capture**, as the facade makes one. Answers this device's id for the
    /// Entry it created.
    fn copied(c: &Connection, user: &str, text: &str) -> i64 {
        copied_at(c, user, text, 1)
    }

    fn copied_at(c: &Connection, user: &str, text: &str, at: i64) -> i64 {
        capture(c, user, b"sealed", text, "this-device", at).unwrap().local_id
    }

    /// The relay taking the next act, exactly as the uploader does it. Answers
    /// the Entry the act belonged to.
    fn flush_next(c: &Connection, user: &str, relay_id: i64, at: i64) -> i64 {
        let act = next_act(c, user).unwrap().expect("an act to settle");
        let entry_id = act.entry_id.expect("every act names its Entry");
        settle(
            c, user, &act,
            Taken::Capture { relay_id, created_at: at, last_use: at },
            at,
        )
        .unwrap();
        entry_id
    }

    fn ids(c: &Connection, user: &str) -> Vec<i64> {
        ids_of(&page(c, user, None, 200).unwrap())
    }

    fn ids_of(rows: &[CachedEntry]) -> Vec<i64> {
        rows.iter().filter_map(|r| r.relay_id).collect()
    }

    fn local_ids(rows: &[CachedEntry]) -> Vec<i64> {
        rows.iter().map(|r| r.local_id).collect()
    }

    fn kinds(c: &Connection, user: &str) -> Vec<ActKind> {
        owed(c, user).unwrap().into_iter().map(|a| a.kind).collect()
    }

    /// What is queued, in the order it will be sent, as the text each act's
    /// Entry holds. No decryption anywhere: the queue holds ciphertext and the
    /// Entry holds the text, and this module is what joins them.
    fn queued_texts(c: &Connection, user: &str) -> Vec<String> {
        owed(c, user)
            .unwrap()
            .into_iter()
            .filter_map(|a| a.entry_id)
            .filter_map(|id| plaintext_of(c, user, id).unwrap())
            .collect()
    }

    /*
     * A capture is one Entry and one act, made together, and this is the only
     * way to make either. Three test helpers used to re-implement the pair —
     * `facade::enqueue_capture`'s, the uploader's and this file's — so a change
     * to the production ordering would have left two of the three suites green.
     */
    #[test]
    fn a_capture_is_an_entry_and_the_act_that_owes_it_to_the_relay() {
        let c = open_in_memory().unwrap();
        let made = capture(&c, "u", b"sealed", "copied offline", "this-phone", 7).unwrap();

        let rows = page(&c, "u", None, 10).unwrap();
        assert_eq!(local_ids(&rows), vec![made.local_id], "the Entry is a row from the moment of capture");
        assert_eq!(rows[0].plaintext.as_deref(), Some("copied offline"));
        assert_eq!(rows[0].relay_id, None, "the relay has not named it");
        assert_eq!((rows[0].created_at, rows[0].last_use), (0, 0), "nor stamped it");
        assert_eq!(rows[0].device_id, "this-phone", "this device is the Origin");
        assert_eq!(rows[0].queued, Queued::Pending);

        let acts = owed(&c, "u").unwrap();
        assert_eq!(
            acts.iter().map(|a| a.entry_id).collect::<Vec<_>>(),
            vec![Some(made.local_id)],
            "and the act names the row it is for, which is what lets it be \
             reconciled onto and what lets it be withdrawn"
        );
        assert_eq!(acts[0].kind, ActKind::Capture(b"sealed".to_vec()));
        assert_eq!(acts[0].captured_at, 7);

        assert_eq!(
            made.change,
            Change { depth: Some(1), reordered: false },
            "one act joined the queue, and nothing changed places"
        );
        assert_eq!(
            recognise(&c, "u", "copied offline").unwrap_or(Held::Nothing),
            Held::Queued(made.local_id),
            "both halves carry the hash, so a repeat copy is recognised against the act"
        );
    }

    /*
     * The whole table, in one place. `PendingCount` used to be emitted from five
     * sites — each preceded by its own queue-depth read — and `HistoryChanged`
     * from seven, so every site was free to get the mapping subtly wrong.
     */
    #[test]
    fn a_change_becomes_events_by_one_table_and_in_one_order() {
        let cases = [
            (Change { depth: None, reordered: false }, Vec::new(), false),
            (Change { depth: Some(2), reordered: false }, vec![2], false),
            (Change::reorder(true), Vec::new(), true),
            (Change { depth: Some(0), reordered: true }, vec![0], true),
        ];
        for (change, counts, reordered) in cases {
            let sink = RecordingSink::default();
            announce(&sink, "u", &change);
            assert_eq!(sink.pending_counts(), counts, "{change:?}");
            assert_eq!(sink.history_changes("u") == 1, reordered, "{change:?}");
        }

        // And when both fire, the order is fixed: a shell that refetches on
        // `HistoryChanged` wants the depth beside the list to be the one that
        // goes with the rows it just asked for.
        let sink = RecordingSink::default();
        announce(&sink, "u", &Change { depth: Some(3), reordered: true });
        let order: Vec<&'static str> = sink
            .events()
            .iter()
            .map(|e| match e {
                CoreEvent::HistoryChanged { .. } => "history",
                CoreEvent::PendingCount { .. } => "count",
                _ => "other",
            })
            .collect();
        assert_eq!(order, vec!["history", "count"]);
    }

    /*
     * What is queued, in order — the question nothing could answer before, so a
     * caller that wanted to know what sat at the head decrypted the queue's own
     * ciphertext to find out.
     */
    #[test]
    fn the_queue_answers_what_is_owed_in_the_order_it_will_be_sent() {
        let c = open_in_memory().unwrap();
        let first = copied_at(&c, "u", "A", 1);
        copied_at(&c, "u", "B", 2);
        copied_at(&c, "u", "C", 3);
        assert_eq!(queued_texts(&c, "u"), vec!["A", "B", "C"]);

        // Re-copying `A` is a fresh act, so it goes to the back — and thereby to
        // the head of the History, which is the same fact read the other way up.
        resend(&c, "u", first, 30).unwrap().unwrap();
        assert_eq!(queued_texts(&c, "u"), vec!["B", "C", "A"]);
        assert_eq!(
            local_ids(&page(&c, "u", None, 10).unwrap())[0],
            first,
            "back of the queue is the head of the History"
        );

        // A refusal is still owed and still in its place. It is simply not
        // deliverable, which is what the next act steps over it for.
        let head = next_act(&c, "u").unwrap().unwrap();
        refuse(&c, "u", &head, 5, "payload too large").unwrap();
        assert_eq!(queued_texts(&c, "u"), vec!["B", "C", "A"], "nothing left the queue");
        assert_eq!(
            plaintext_of(&c, "u", next_act(&c, "u").unwrap().unwrap().entry_id.unwrap())
                .unwrap()
                .as_deref(),
            Some("C"),
            "and the act behind the refusal is the one now offered"
        );
    }

    /*
     * The two rules five doc comments used to state and nothing enforced in one
     * place. A move is not a depth change: the act moved, it did not multiply.
     * A flush is not a reorder: the relay stamps a pending act exactly where the
     * device already showed it.
     */
    #[test]
    fn a_move_is_not_a_depth_change_and_a_flush_is_not_a_reorder() {
        let c = open_in_memory().unwrap();
        let now = crate::now_ms();
        let first = copied_at(&c, "u", "first", 1);
        copied_at(&c, "u", "second", 2);

        let moved = resend(&c, "u", first, 3).unwrap().unwrap();
        assert_eq!(
            moved,
            Change { depth: None, reordered: true },
            "a move reorders the list and leaves the queue exactly as deep"
        );
        assert_eq!(depth(&c, "u").unwrap(), 2, "moved, not duplicated");

        let before = local_ids(&page(&c, "u", None, 10).unwrap());
        let act = next_act(&c, "u").unwrap().unwrap();
        let settled = settle(
            &c, "u", &act,
            Taken::Capture { relay_id: 42, created_at: now, last_use: now },
            now,
        )
        .unwrap();
        assert_eq!(
            settled.change,
            Change { depth: Some(1), reordered: false },
            "a flush takes one act off the queue and moves nothing"
        );
        assert_eq!(
            local_ids(&page(&c, "u", None, 10).unwrap()),
            before,
            "the same rows in the same places, either side of the flush"
        );
    }

    // -- the order --------------------------------------------------------

    #[test]
    fn list_returns_last_used_first() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 { ins(&c, "u", i, Some(&format!("p{i}")), 1000 + i, 9_999); }
        assert_eq!(ids(&c, "u"), vec![3, 2, 1]);
    }

    /// The whole point of the ordering: an old capture that was just used leads
    /// a history of entries captured since.
    #[test]
    fn a_use_moves_an_old_entry_to_the_head() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 { ins(&c, "u", i, Some(&format!("p{i}")), 1000 + i, 9_999); }
        assert!(record_use(&c, "u", 1, 5_000).unwrap().reordered);
        assert_eq!(ids(&c, "u"), vec![1, 3, 2]);
    }

    /// Recalling the entry already at the head cannot move the order. It renews
    /// tenure, which is the half that matters to the age cap.
    #[test]
    fn using_the_head_renews_it_without_reordering() {
        let c = open_in_memory().unwrap();
        for i in 1..=3 { ins(&c, "u", i, None, 1000 + i, 9_999); }
        record_use(&c, "u", 3, 8_000).unwrap();
        assert_eq!(ids(&c, "u"), vec![3, 2, 1]);
        let head = page(&c, "u", None, 1).unwrap().pop().unwrap();
        assert_eq!(head.last_use, 8_000);
        assert_eq!(head.created_at, 1003, "a use leaves identity alone");
    }

    #[test]
    fn a_use_of_an_entry_this_device_does_not_hold_moves_nothing() {
        let c = open_in_memory().unwrap();
        assert_eq!(record_use(&c, "u", 404, 1).unwrap(), Change::reorder(false));
    }

    #[test]
    fn paging_with_a_cursor_taken_off_the_last_row_shown() {
        let c = open_in_memory().unwrap();
        for i in 1..=10 { ins(&c, "u", i, None, i, 9_999); }
        let first = page(&c, "u", None, 3).unwrap();
        assert_eq!(ids_of(&first), vec![10, 9, 8]);
        let next = page(&c, "u", Some(first.last().unwrap().resume_from), 3).unwrap();
        assert_eq!(ids_of(&next), vec![7, 6, 5]);
    }

    /// Two entries sharing a millisecond still page without skipping or
    /// repeating, which is the whole reason the cursor carries the id.
    #[test]
    fn paging_is_total_when_two_entries_share_a_last_use() {
        let c = open_in_memory().unwrap();
        for i in 1..=4 { used(&c, "u", i, None, i, 500, 9_999); }
        let first = page(&c, "u", None, 2).unwrap();
        assert_eq!(ids_of(&first), vec![4, 3]);
        let next = page(&c, "u", Some(first.last().unwrap().resume_from), 2).unwrap();
        assert_eq!(ids_of(&next), vec![2, 1]);
    }

    /*
     * ADR 0014, as one list. The pending acts sit above the entries the relay has
     * ordered, in the order this device will send them, and the seam does not move
     * at the flush — the relay stamps a pending act exactly where the device
     * already showed it.
     */
    #[test]
    fn the_order_is_identical_either_side_of_a_flush() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        // Two the relay already has, oldest first.
        let old = ins(&c, "u", 1, Some("a week ago"), now - 10_000, now);
        let newer = ins(&c, "u", 2, Some("yesterday"), now - 5_000, now);
        // And a burst made with no relay in reach.
        let one = copied(&c, "u", "burst one");
        let two = copied(&c, "u", "burst two");
        let three = copied(&c, "u", "burst three");

        let offline = page(&c, "u", None, 50).unwrap();
        assert_eq!(
            local_ids(&offline),
            vec![three, two, one, newer, old],
            "the queue, newest act first, above what the relay has ordered"
        );
        assert_eq!(
            offline.iter().map(|r| r.queued.clone()).collect::<Vec<_>>(),
            vec![
                Queued::Pending, Queued::Pending, Queued::Pending,
                Queued::Settled, Queued::Settled,
            ]
        );

        // The relay takes them in queue order and stamps each one as it arrives,
        // which is what a flush is.
        for n in 0..3 {
            flush_next(&c, "u", 100 + n, now + n);
        }

        let flushed = page(&c, "u", None, 50).unwrap();
        assert_eq!(
            local_ids(&flushed),
            local_ids(&offline),
            "the flush moved nothing: the same rows in the same places"
        );
        assert!(flushed.iter().all(|r| r.queued == Queued::Settled));
    }

    /*
     * A repeat copy matches captures head-first, so two offline re-copies of one
     * entry enqueue two acts against it. The *latest* act is what the row sorts
     * by, which is what makes it rise on the second one.
     */
    #[test]
    fn an_entry_re_copied_offline_twice_stays_at_the_top() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        let old = ins(&c, "u", 1, Some("three weeks old"), now - 100_000, now);
        let later = copied(&c, "u", "captured after it");

        // Re-copying the old entry queues a use against it, which lifts it above
        // the capture made since.
        queue_use(&c, "u", old, 1, 2).unwrap();
        assert_eq!(local_ids(&page(&c, "u", None, 50).unwrap()), vec![old, later]);

        // Something else is copied, and then the old entry again.
        let newest = copied(&c, "u", "and something else");
        assert_eq!(local_ids(&page(&c, "u", None, 50).unwrap()), vec![newest, old, later]);
        queue_use(&c, "u", old, 1, 3).unwrap();
        assert_eq!(
            local_ids(&page(&c, "u", None, 50).unwrap()),
            vec![old, newest, later],
            "the second re-copy has to lift it again, which the first act cannot say"
        );
    }

    /*
     * The seam is a page boundary like any other: a cursor taken from the last
     * pending row hands back the first settled one, with no gap and no repeat.
     */
    #[test]
    fn paging_crosses_the_seam_with_no_gap_and_no_repeat() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        let settled: Vec<i64> =
            (1..=3).map(|i| ins(&c, "u", i, None, now - (10 - i) * 100, now)).collect();
        let pending: Vec<i64> = (0..2).map(|i| copied(&c, "u", &format!("queued {i}"))).collect();

        let whole = local_ids(&page(&c, "u", None, 50).unwrap());
        let mut paged = Vec::new();
        let mut cursor_at = None;
        loop {
            let one = page(&c, "u", cursor_at, 2).unwrap();
            if one.is_empty() {
                break;
            }
            cursor_at = Some(one.last().unwrap().resume_from);
            paged.extend(local_ids(&one));
        }
        assert_eq!(paged, whole, "paging two at a time reads exactly the same list");
        assert_eq!(paged.len(), pending.len() + settled.len());
    }

    /*
     * A refusal leads the History, above every act still on its way, because it
     * is the actionable one.
     */
    #[test]
    fn a_refused_act_leads_the_history_and_carries_its_reason() {
        let c = open_in_memory().unwrap();
        let refused = copied(&c, "u", "too large for this relay");
        let live = copied(&c, "u", "still on its way");
        let head = next_act(&c, "u").unwrap().unwrap();
        assert_eq!(head.entry_id, Some(refused), "precondition: the oldest act is the head");
        assert!(refuse(&c, "u", &head, 5, "payload too large").unwrap().claimed);

        let rows = page(&c, "u", None, 50).unwrap();
        assert_eq!(local_ids(&rows), vec![refused, live]);
        assert_eq!(rows[0].queued, Queued::Refused("payload too large".into()));
        assert_eq!(rows[1].queued, Queued::Pending);
    }

    // -- the caps ---------------------------------------------------------

    #[test]
    fn caps_at_max_per_user() {
        let c = open_in_memory().unwrap();
        for i in 1..=105 { ins(&c, "u", i, None, i, 100_000); }
        let rows = page(&c, "u", None, 200).unwrap();
        assert_eq!(rows.len() as i64, MAX_PER_USER);
        assert_eq!(rows.first().unwrap().relay_id, Some(105));
        assert_eq!(rows.last().unwrap().relay_id, Some(6));
    }

    /// The count cap measures from Last Use, so the row sitting at the top of
    /// the list is never the one evicted to make room.
    #[test]
    fn the_count_cap_keeps_a_used_entry_and_evicts_an_unused_newer_one() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, None, 1, 100_000);
        for i in 2..=MAX_PER_USER { ins(&c, "u", i, None, i, 100_000); }
        record_use(&c, "u", 1, 90_000).unwrap();
        // One more capture: something has to go, and it must not be entry 1.
        ins(&c, "u", 500, None, 500, 100_000);

        let kept = ids(&c, "u");
        assert_eq!(kept.len() as i64, MAX_PER_USER);
        assert_eq!(kept.first(), Some(&1), "the entry last used leads the history");
        assert!(!kept.contains(&2), "the least recently used entry is the one evicted");
    }

    #[test]
    fn evicts_old_by_age() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        ins(&c, "u", 1, None, now - MAX_AGE_MS - 1, now);
        ins(&c, "u", 2, None, now, now);
        assert_eq!(ids(&c, "u"), vec![2]);
    }

    /// Thirty days now means thirty days since last use, so ciphertext in
    /// regular use is never aged out.
    #[test]
    fn the_age_cap_measures_from_last_use() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        used(&c, "u", 1, None, now - MAX_AGE_MS - 1, now - 1000, now);
        ins(&c, "u", 2, None, now, now);
        assert_eq!(ids(&c, "u"), vec![2, 1], "an ancient capture used a second ago survives");
    }

    /*
     * A hundred and fifty offline captures are a hundred and fifty rows: the caps
     * bound what the relay has ordered, and an act still owed to the relay is
     * undelivered content. After the flush the settled region is a hundred and the
     * ones that fell off are the oldest.
     */
    #[test]
    fn a_hundred_and_fifty_offline_captures_are_a_hundred_and_fifty_rows() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        let ids: Vec<i64> = (0..150).map(|i| copied(&c, "u", &format!("copy {i}"))).collect();
        assert_eq!(page(&c, "u", None, MAX_PAGE).unwrap().len(), 150);

        for n in 0..150 {
            flush_next(&c, "u", 1_000 + n, now + n);
        }

        let after = page(&c, "u", None, MAX_PAGE).unwrap();
        assert_eq!(after.len() as i64, MAX_PER_USER, "the settled region is capped");
        let kept = local_ids(&after);
        assert_eq!(kept.first(), Some(&ids[149]), "newest first");
        assert_eq!(kept.last(), Some(&ids[50]), "and the fifty oldest fell off");
    }

    /*
     * The caps bound what the relay has ordered. An act still owed to the relay
     * is undelivered clipboard content, and evicting one to protect a display
     * invariant is the trade ADR 0014 refuses — so a hundred and one offline
     * captures are a hundred and one rows.
     */
    #[test]
    fn an_un_flushed_capture_is_never_evicted() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        let first = copied(&c, "u", "the first offline copy");
        for i in 1..=MAX_PER_USER + 20 {
            ins(&c, "u", i, None, now, now);
        }
        // Counted off the table rather than a page, because what is under test is
        // what survives eviction rather than what one page shows.
        assert_eq!(
            plaintext_of(&c, "u", first).unwrap().as_deref(),
            Some("the first offline copy"),
            "the un-flushed capture was evicted by rows the relay had already ordered"
        );
        let settled: i64 = c
            .query_row(
                "SELECT COUNT(*) FROM entries_cache WHERE user_id = 'u' AND relay_id IS NOT NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(settled, MAX_PER_USER, "and the settled region is still capped at a hundred");

        // The age cap is the same rule: nothing to measure, nothing to evict.
        ins(&c, "u", 9_999, None, now + MAX_AGE_MS * 2, now + MAX_AGE_MS * 2);
        assert!(plaintext_of(&c, "u", first).unwrap().is_some());
    }

    /*
     * The exemption a `relay_id IS NULL` test would miss, and the case that shows
     * why it matters. An entry the relay *has* named but that carries a queued
     * **Use** is at the top of the list — and it holds the *lowest* `last_use` in
     * the cache, because that is what being three weeks old means. Counting it
     * against the hundred would evict exactly the row the person just reached
     * for, and strand its act in the queue with nothing to reconcile onto.
     */
    #[test]
    fn an_entry_with_a_queued_use_is_never_evicted_either() {
        let c = open_in_memory().unwrap();
        let now = 100_000_000_000_i64;
        // Three weeks old, and the oldest thing in the cache.
        let ancient = ins(&c, "u", 1, Some("wg genkey"), now - 21 * 86_400_000, now);
        // Re-copied with no relay in reach, which queues a use against it.
        queue_use(&c, "u", ancient, 1, now).unwrap();

        // A full cache of things the relay has ordered since, then one more.
        for i in 2..=MAX_PER_USER + 1 {
            ins(&c, "u", i, None, now - MAX_PER_USER + i, now);
        }

        let rows = page(&c, "u", None, MAX_PAGE).unwrap();
        assert_eq!(rows[0].local_id, ancient, "the re-copied entry leads the History");
        assert_eq!(rows[0].queued, Queued::Pending);
        assert_eq!(
            plaintext_of(&c, "u", ancient).unwrap().as_deref(),
            Some("wg genkey"),
            "and survived every prune the hundred rows after it provoked"
        );
        assert_eq!(
            rows.iter().filter(|r| r.queued == Queued::Settled).count() as i64,
            MAX_PER_USER,
            "while the settled region is still capped at exactly a hundred"
        );

        // Once the relay takes the use, the row is settled like any other and
        // takes its chances with the cap — which is the whole point of the
        // exemption being about what is *owed* rather than about the row.
        let act = next_act(&c, "u").unwrap().unwrap();
        settle(&c, "u", &act, Taken::Use { relay_id: 1, last_use: now }, now).unwrap();
        ins(&c, "u", 9_000, None, now, now);
        assert!(
            plaintext_of(&c, "u", ancient).unwrap().is_some(),
            "and the relay's stamp is what keeps it, now that it competes"
        );
    }

    /*
     * The queue used to evict its oldest acts at a cap of a thousand, silently
     * destroying clipboard content that had reached nowhere else to keep a number
     * down (ADR 0014). A thousand and five offline acts are a thousand and five.
     */
    #[test]
    fn nothing_is_ever_evicted_from_the_queue() {
        let c = open_in_memory().unwrap();
        for i in 0..1_005 {
            copied_at(&c, "u", "x", i);
        }
        for i in 0..3 {
            copied_at(&c, "v", "x", i);
        }
        assert_eq!(depth(&c, "u").unwrap(), 1_005);
        assert_eq!(depth(&c, "v").unwrap(), 3);
    }

    // -- the queue --------------------------------------------------------

    /// The reason there is one queue: an outage cannot reorder what happened
    /// during it.
    #[test]
    fn captures_and_uses_are_sent_in_the_order_they_were_made() {
        let c = open_in_memory().unwrap();
        copied_at(&c, "u", "first", 1);
        queue_use(&c, "u", 9, 42, 2).unwrap();
        copied_at(&c, "u", "second", 3);

        assert_eq!(
            kinds(&c, "u"),
            vec![
                ActKind::Capture(b"sealed".to_vec()),
                ActKind::Use(42),
                ActKind::Capture(b"sealed".to_vec()),
            ]
        );
        assert_eq!(next_act(&c, "u").unwrap().unwrap().captured_at, 1, "head-first");
    }

    /// Both kinds name the entry they belong to, and the link survives a move to
    /// the back: everything that finds an act from its row depends on it.
    #[test]
    fn both_kinds_carry_the_entry_they_belong_to() {
        let c = open_in_memory().unwrap();
        let captured = copied(&c, "u", "ct");
        assert_eq!(next_act(&c, "u").unwrap().unwrap().entry_id, Some(captured));
        forget_entry(&c, "u", captured).unwrap();

        queue_use(&c, "u", 12, 500, 2).unwrap();
        assert_eq!(next_act(&c, "u").unwrap().unwrap().entry_id, Some(12));
        resend(&c, "u", 12, 30).unwrap().unwrap();
        let moved = next_act(&c, "u").unwrap().unwrap();
        assert_eq!(moved.entry_id, Some(12), "the link is a fresh act's too");
        assert_eq!(moved.kind, ActKind::Use(500), "and so is the relay's id");
    }

    /// The moved pending is a fresh act, so it is still recognisable and no
    /// longer carries the failures of its previous position.
    #[test]
    fn a_moved_act_keeps_its_hash_and_drops_its_attempts() {
        let c = open_in_memory().unwrap();
        let first = copied_at(&c, "u", "first", 1);
        let act = next_act(&c, "u").unwrap().unwrap();
        record_failure(&c, &act, "boom").unwrap();
        resend(&c, "u", first, 30).unwrap().unwrap();

        assert!(
            matches!(recognise(&c, "u", "first").unwrap(), Held::Queued(id) if id == first),
            "a fresh act carries the hash, so a repeat copy still recognises it"
        );
        let moved = next_act(&c, "u").unwrap().unwrap();
        assert_eq!(moved.attempts, 0);
        assert_eq!(moved.last_error, None);
        assert_eq!(moved.captured_at, 30);
    }

    #[test]
    fn record_failure_counts_attempts_and_keeps_what_the_relay_said() {
        let c = open_in_memory().unwrap();
        copied(&c, "u", "x");
        let act = next_act(&c, "u").unwrap().unwrap();
        record_failure(&c, &act, "boom").unwrap();
        assert_eq!(next_act(&c, "u").unwrap().unwrap().attempts, 1);
        record_failure(&c, &act, "again").unwrap();
        let act = next_act(&c, "u").unwrap().unwrap();
        assert_eq!(act.attempts, 2);
        assert_eq!(act.last_error.as_deref(), Some("again"));
        assert!(
            next_act(&c, "u").unwrap().is_some(),
            "and the act is still deliverable: unreachable is never refused"
        );
    }

    /// One act earns one refusal, however many uploaders are told about it.
    #[test]
    fn a_refusal_is_claimed_once() {
        let c = open_in_memory().unwrap();
        copied(&c, "u", "too large");
        let act = next_act(&c, "u").unwrap().unwrap();
        let first = refuse(&c, "u", &act, 1, "payload too large").unwrap();
        assert!(first.claimed);
        assert_eq!(
            first.change,
            Change { depth: Some(1), reordered: false },
            "nothing is deleted: the act is still on this device"
        );
        assert!(!refuse(&c, "u", &act, 2, "again").unwrap().claimed);
        assert!(next_act(&c, "u").unwrap().is_none(), "and it is no longer deliverable");
    }

    /*
     * The withdrawal race: the upload awaited with the lock released, and a
     * delete inside that window took the queued act with it. The relay has the
     * Entry by then, so the only honest outcome is to take it back off the relay.
     */
    #[test]
    fn an_act_withdrawn_during_its_upload_is_reported_rather_than_reconciled() {
        let c = open_in_memory().unwrap();
        let now = crate::now_ms();
        let entry = copied(&c, "u", "withdrawn mid-flight");
        let act = next_act(&c, "u").unwrap().unwrap();
        forget_entry(&c, "u", entry).unwrap();

        let settled = settle(
            &c, "u", &act,
            Taken::Capture { relay_id: 500, created_at: now, last_use: now },
            now,
        )
        .unwrap();
        assert_eq!(settled.withdrawn, Some(500));
        assert_eq!(settled.stamp, None, "there is no row left to stamp");
        assert!(page(&c, "u", None, 10).unwrap().is_empty());
    }

    /// What the settlement carries, per kind of act. `None` is the relay saying
    /// nothing rather than this device not knowing.
    #[test]
    fn a_settlement_carries_what_the_relay_stamped_and_no_more() {
        let c = open_in_memory().unwrap();
        let now = crate::now_ms();
        let entry = copied(&c, "u", "captured offline");
        let act = next_act(&c, "u").unwrap().unwrap();
        let out = settle(
            &c, "u", &act,
            Taken::Capture { relay_id: 42, created_at: now, last_use: now },
            now,
        )
        .unwrap();
        assert_eq!(
            out.stamp,
            Some(Stamp { created_at: Some(now), last_use: Some(now) }),
            "a capture is stamped for the first time"
        );

        queue_use(&c, "u", entry, 42, 2).unwrap();
        let act = next_act(&c, "u").unwrap().unwrap();
        let out = settle(&c, "u", &act, Taken::Use { relay_id: 42, last_use: now + 1 }, now).unwrap();
        assert_eq!(
            out.stamp,
            Some(Stamp { created_at: None, last_use: Some(now + 1) }),
            "a use does not restamp the creation"
        );

        queue_use(&c, "u", entry, 42, 3).unwrap();
        let act = next_act(&c, "u").unwrap().unwrap();
        let out = settle(&c, "u", &act, Taken::UseVanished { relay_id: 42 }, now).unwrap();
        assert_eq!(
            out.stamp,
            Some(Stamp { created_at: None, last_use: None }),
            "a vanished use stamps nothing, and the row has still stopped waiting"
        );
        assert_eq!(depth(&c, "u").unwrap(), 0);
    }

    // -- what this device already holds -----------------------------------

    /// The queue first, and only then the entries: an un-flushed capture has no
    /// relay id to record a use against.
    #[test]
    fn a_queued_capture_is_recognised_before_a_settled_entry_and_a_use_never_is() {
        let c = open_in_memory().unwrap();
        let queued = copied(&c, "u", "hello");
        queue_use(&c, "u", 9, 7, 2).unwrap();
        assert!(matches!(recognise(&c, "u", "hello").unwrap(), Held::Queued(id) if id == queued));
        assert!(matches!(recognise(&c, "u", "hello\n").unwrap(), Held::Nothing));
        assert!(matches!(recognise(&c, "other", "hello").unwrap(), Held::Nothing));

        // Once the relay has taken it, the act is gone and the entry answers.
        let act = next_act(&c, "u").unwrap().unwrap();
        let now = crate::now_ms();
        settle(&c, "u", &act, Taken::Capture { relay_id: 5, created_at: now, last_use: now }, now)
            .unwrap();
        assert!(matches!(recognise(&c, "u", "hello").unwrap(), Held::Entry(id) if id == queued));
    }

    #[test]
    fn recognition_matches_exact_bytes_only() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("https://example.test"), 1, 9);
        assert!(matches!(recognise(&c, "u", "https://example.test").unwrap(), Held::Entry(1)));
        assert!(
            matches!(recognise(&c, "u", "https://example.test\n").unwrap(), Held::Nothing),
            "a trailing newline makes it a different entry"
        );
        assert!(matches!(recognise(&c, "other", "https://example.test").unwrap(), Held::Nothing));
    }

    #[test]
    fn a_ciphertext_only_re_ingest_keeps_the_plaintext_and_the_hash() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("first"), 1, 9);
        ins(&c, "u", 1, None, 2, 9);
        assert_eq!(plaintext_of(&c, "u", 1).unwrap().as_deref(), Some("first"));
        assert!(matches!(recognise(&c, "u", "first").unwrap(), Held::Entry(1)));
    }

    #[test]
    fn mark_undecryptable_clears_plaintext_and_stops_it_matching() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 1, Some("secret"), 1, 9);
        mark_undecryptable(&c, "u", 1).unwrap();
        assert_eq!(plaintext_of(&c, "u", 1).unwrap(), None);
        assert!(matches!(recognise(&c, "u", "secret").unwrap(), Held::Nothing));
    }

    // -- identity and forgetting ------------------------------------------

    /*
     * An Entry exists before the relay names it (ADR 0016), so a capture is a
     * row from the moment it is made — findable, readable and withdrawable
     * there, with no relay id and nothing the caps can measure.
     */
    #[test]
    fn a_capture_is_a_row_before_the_relay_names_it() {
        let c = open_in_memory().unwrap();
        let local_id = copied(&c, "u", "offered offline");
        let now = crate::now_ms();

        assert_eq!(relay_id_of(&c, "u", local_id).unwrap(), None, "the relay has not named it");
        assert!(
            matches!(relay_id_of(&c, "u", local_id + 999), Err(AppError::NotFound(_))),
            "and a row that is not here is not the same answer as one the relay has not named"
        );

        // The relay takes it: same row, same id, now stamped.
        let act = next_act(&c, "u").unwrap().unwrap();
        settle(&c, "u", &act, Taken::Capture { relay_id: 77, created_at: now, last_use: now }, now)
            .unwrap();
        let settled = page(&c, "u", None, 10).unwrap();
        assert_eq!(settled.len(), 1, "reconciliation, not insertion");
        assert_eq!(settled[0].local_id, local_id);
        assert_eq!(settled[0].relay_id, Some(77));
        assert_eq!(settled[0].created_at, now);
        assert_eq!(relay_id_of(&c, "u", local_id).unwrap(), Some(77));
    }

    /// The local id is the cache's own, and does not become the relay's by
    /// coincidence: it is dense per machine while relay ids are not.
    #[test]
    fn every_row_has_a_local_id_of_its_own() {
        let c = open_in_memory().unwrap();
        ins(&c, "u", 40, Some("first"), 1, 9);
        ins(&c, "u", 90, Some("second"), 2, 9);
        let rows = page(&c, "u", None, 10).unwrap();
        assert_eq!(ids_of(&rows), vec![90, 40]);
        assert_eq!(local_ids(&rows), vec![2, 1], "local ids follow insertion, not the relay");

        // And re-ingesting leaves it where it was: a shell's row key must not
        // move because the relay said something about the row again.
        ins(&c, "u", 40, Some("first"), 1, 9);
        let again = page(&c, "u", None, 10).unwrap();
        let held = again.iter().find(|r| r.relay_id == Some(40)).unwrap();
        assert_eq!(held.local_id, 1);
    }

    /*
     * A relay id names one row *per Pairing* and not per machine. Two pairings
     * on one device are two relays' numbering, and nothing says they do not
     * collide — a device paired to two relays would otherwise have one pairing's
     * entry overwrite the other's.
     */
    #[test]
    fn a_relay_id_is_unique_within_a_pairing_and_not_across_them() {
        let c = open_in_memory().unwrap();
        let theirs = ins(&c, "a", 1, Some("theirs"), 1, 9);
        let mine = ins(&c, "b", 1, Some("mine"), 1, 9);
        assert_ne!(theirs, mine, "one relay id, two rows");
        assert_eq!(plaintext_of(&c, "a", theirs).unwrap().as_deref(), Some("theirs"));
        assert_eq!(plaintext_of(&c, "b", mine).unwrap().as_deref(), Some("mine"));

        // Re-ingesting the same pairing's same relay id updates that one row.
        assert_eq!(ins(&c, "a", 1, Some("theirs, again"), 2, 9), theirs);
        assert_eq!(page(&c, "a", None, 10).unwrap().len(), 1);
        assert_eq!(plaintext_of(&c, "a", theirs).unwrap().as_deref(), Some("theirs, again"));
    }

    /// Withdrawing an un-named row takes its act with it; a row the relay has
    /// named leaves the queue alone, because the caller has already told the
    /// relay.
    #[test]
    fn forgetting_an_entry_withdraws_the_act_only_when_the_relay_never_named_it() {
        let c = open_in_memory().unwrap();
        let mistake = copied(&c, "u", "a mistake");
        copied(&c, "u", "keep this");
        assert_eq!(
            forget_entry(&c, "u", mistake).unwrap(),
            Change { depth: Some(1), reordered: false },
            "the act went with the row, and the count chrome is what says so"
        );
        assert_eq!(queued_texts(&c, "u"), vec!["keep this"]);

        let named = ins(&c, "u", 7, Some("the relay has this"), 1, 9);
        assert_eq!(
            forget_entry(&c, "u", named).unwrap(),
            Change { depth: None, reordered: false },
            "nothing left the queue, so nothing reports a depth"
        );
        assert_eq!(page(&c, "u", None, 10).unwrap().len(), 1);
    }

    #[test]
    fn the_relay_deleting_an_entry_resolves_it_to_this_devices_own_id() {
        let c = open_in_memory().unwrap();
        let local_id = ins(&c, "u", 7, Some("deleted elsewhere"), 1, 9);
        assert_eq!(forget_relay_entry(&c, "u", 7).unwrap(), Some(local_id));
        assert!(page(&c, "u", None, 10).unwrap().is_empty());
        assert_eq!(forget_relay_entry(&c, "u", 7).unwrap(), None, "and again is nothing");
    }

    /*
     * `clear_history` empties the queue with the entries: a queue left standing
     * would repopulate exactly what was just cleared on the next flush.
     * `forget_pairing` does not, because the Pairing itself is going away.
     */
    #[test]
    fn clearing_a_history_empties_its_queue_and_forgetting_a_pairing_does_not() {
        let c = open_in_memory().unwrap();
        copied(&c, "a", "queued and then cleared");
        ins(&c, "b", 1, None, 1, 9);
        assert_eq!(
            forget_all(&c, "a").unwrap(),
            Change { depth: Some(0), reordered: true },
        );
        assert!(page(&c, "a", None, 10).unwrap().is_empty());
        assert_eq!(page(&c, "b", None, 10).unwrap().len(), 1, "one user at a time");

        copied(&c, "a", "queued and then unpaired");
        forget_entries(&c, "a").unwrap();
        assert!(page(&c, "a", None, 10).unwrap().is_empty());
        assert_eq!(depth(&c, "a").unwrap(), 1, "the Pairing is going; the rows are not its queue");
    }
}

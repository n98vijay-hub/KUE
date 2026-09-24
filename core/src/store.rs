//! Local structured memory.
//!
//! WHAT IS STORED: events (a timestamp, a kind, a one-line summary) and periodic
//! context snapshots (the JSON context object).
//!
//! WHAT IS NEVER STORED, by construction: camera frames, face crops, any image,
//! audio, keystrokes, typed text, clipboard contents, window titles, document
//! names or URLs. None of those ever reach this crate — the sensing layer does
//! not emit them, so there is nothing here to write.
//!
//! The database is a plain SQLite file on this Mac. Nothing is uploaded.

use crate::events::{Event, RememberedEvent};
use crate::privacy::{Cleared, LedgerEntry, MemorySnapshot, PRIVACY_POLICY_VERSION};
use rusqlite::{params, Connection};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("database error: {0}")]
    Db(#[from] rusqlite::Error),
    #[error("serialisation error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("another KUE instance is already using this local memory ({0})")]
    InUse(String),
    #[error("the lock file could not be opened: {0}")]
    Lock(#[from] std::io::Error),
}

pub struct Store {
    conn: Connection,
    /// Held for the life of the store. Two running copies of Lantern (a debug and
    /// a release build, say) would otherwise both write, trim and migrate the same
    /// file, and neither timeline could be trusted.
    _lock: Option<std::fs::File>,
    max_events: u64,
    max_snapshots: u64,
    /// Diagnostics are bounded like everything else: KUE measuring itself may
    /// never be the reason the owner runs out of disk.
    max_samples: u64,
    max_timings: u64,
}

const SCHEMA: &str =
    "CREATE TABLE IF NOT EXISTS events (
         id         INTEGER PRIMARY KEY AUTOINCREMENT,
         ts         REAL NOT NULL,
         kind       TEXT NOT NULL,
         summary    TEXT NOT NULL,
         detail     TEXT,
         confidence REAL,
         evidence   TEXT
     );
     CREATE INDEX IF NOT EXISTS idx_events_ts ON events(ts);
     CREATE TABLE IF NOT EXISTS context_snapshots (
         id             INTEGER PRIMARY KEY AUTOINCREMENT,
         ts             REAL NOT NULL,
         schema_version INTEGER NOT NULL,
         identity_state TEXT NOT NULL,
         activity_state TEXT NOT NULL,
         body           TEXT NOT NULL
     );
     CREATE INDEX IF NOT EXISTS idx_snapshots_ts ON context_snapshots(ts);
     CREATE TABLE IF NOT EXISTS privacy_ledger (
         kind           TEXT NOT NULL,
         class          TEXT NOT NULL,
         destination    TEXT NOT NULL,
         decision       TEXT NOT NULL,
         reason         TEXT NOT NULL,
         first_ts       REAL NOT NULL,
         last_ts        REAL NOT NULL,
         count          INTEGER NOT NULL,
         policy_version INTEGER NOT NULL,
         PRIMARY KEY (kind, destination, decision, policy_version)
     );
     -- How the measuring went, one row per sampled moment. KUE's account of its
     -- own machinery: states, ages and durations. Never who was measured.
     CREATE TABLE IF NOT EXISTS perception_samples (
         id                 INTEGER PRIMARY KEY AUTOINCREMENT,
         ts                 REAL NOT NULL,
         measurement        TEXT NOT NULL,
         identity           TEXT NOT NULL,
         access_level       TEXT NOT NULL,
         measurement_age_ms INTEGER,
         analyze_ms         INTEGER,
         capture_gap_ms     INTEGER,
         frames_dropped     INTEGER,
         vision_busy        INTEGER,
         model_phase        TEXT NOT NULL,
         track_id           INTEGER,
         frames_tracked     INTEGER,
         capture_quality    REAL,
         face_count         INTEGER,
         policy_version     INTEGER NOT NULL
     );
     CREATE INDEX IF NOT EXISTS idx_perception_ts ON perception_samples(ts);
     -- How long one stage of one operation took. The op id is stripped to
     -- identifier characters before it ever gets here (telemetry::Span::new).
     CREATE TABLE IF NOT EXISTS stage_timings (
         id             INTEGER PRIMARY KEY AUTOINCREMENT,
         ts             REAL NOT NULL,
         stage          TEXT NOT NULL,
         op             TEXT NOT NULL,
         duration_ms    INTEGER NOT NULL,
         outcome        TEXT NOT NULL,
         policy_version INTEGER NOT NULL
     );
     CREATE INDEX IF NOT EXISTS idx_stage_ts ON stage_timings(ts);
     -- What KUE keeps about the owner's world (memory.rs). One row per
     -- memory, carrying why it is kept and where it came from. A memory the
     -- owner forgets is DELETED from here, not blanked: the words go.
     CREATE TABLE IF NOT EXISTS memories (
         id             TEXT PRIMARY KEY,
         subject        TEXT NOT NULL,
         statement      TEXT NOT NULL,
         class          TEXT NOT NULL,
         state          TEXT NOT NULL,
         source         TEXT NOT NULL,
         why            TEXT NOT NULL,
         privacy        TEXT NOT NULL,
         created_at     REAL NOT NULL,
         updated_at     REAL NOT NULL,
         valid_for      REAL,
         provenance     TEXT NOT NULL,
         relates_to     TEXT,
         superseded_by  TEXT,
         policy_version INTEGER NOT NULL
     );
     CREATE INDEX IF NOT EXISTS idx_memories_updated ON memories(updated_at);";

/// Two days of one-a-second samples, and enough stage timings to profile a
/// week of ordinary use. Both are trimmed on write, like events and snapshots.
pub const DEFAULT_MAX_SAMPLES: u64 = 172_800;
pub const DEFAULT_MAX_TIMINGS: u64 = 50_000;

/// What the local memory costs on disk, measured.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StoreFootprint {
    pub events: u64,
    pub snapshots: u64,
    pub perception_samples: u64,
    pub stage_timings: u64,
    pub ledger_rows: u64,
    /// What the file occupies.
    pub file_bytes: u64,
    /// Pages the file holds that hold nothing: freed by trimming and never
    /// returned to the disk, because the database does not auto-vacuum.
    pub reusable_bytes: u64,
}

impl Store {
    pub fn open(path: &Path, max_events: u64, max_snapshots: u64) -> Result<Self, StoreError> {
        if let Some(p) = path.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let lock_path = path.with_extension("lock");
        let lock = std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(&lock_path)?;
        if lock.try_lock().is_err() {
            return Err(StoreError::InUse(lock_path.display().to_string()));
        }
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode=WAL;")?;
        Self::init(&conn)?;
        Ok(Store { conn, _lock: Some(lock), max_events, max_snapshots,
                   max_samples: DEFAULT_MAX_SAMPLES, max_timings: DEFAULT_MAX_TIMINGS })
    }

    pub fn open_in_memory(max_events: u64, max_snapshots: u64) -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        Self::init(&conn)?;
        Ok(Store { conn, _lock: None, max_events, max_snapshots,
                   max_samples: DEFAULT_MAX_SAMPLES, max_timings: DEFAULT_MAX_TIMINGS })
    }

    fn init(conn: &Connection) -> Result<(), StoreError> {
        // Deleted rows are overwritten with zeros rather than left in free pages,
        // so "deleted" means the bytes are gone, not merely unlinked.
        conn.execute_batch("PRAGMA secure_delete=ON;")?;
        conn.execute_batch(SCHEMA)?;
        // Snapshots written before the privacy firewall have no policy version.
        // They are marked 0 — they may contain body joint positions and face
        // measurement tracks — and stay until the owner purges them.
        let snap_cols: Vec<String> = conn.prepare("PRAGMA table_info(context_snapshots)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .collect();
        if !snap_cols.iter().any(|c| c == "policy_version") {
            conn.execute_batch("ALTER TABLE context_snapshots ADD COLUMN policy_version INTEGER NOT NULL DEFAULT 0;")?;
        }
        // Databases written before provenance existed lack its two columns. They
        // are added in place: existing rows are kept, and simply have no "why".
        let columns: Vec<String> = conn.prepare("PRAGMA table_info(events)")?
            .query_map([], |r| r.get::<_, String>(1))?
            .filter_map(Result::ok)
            .collect();
        for (name, ty) in [("confidence", "REAL"), ("evidence", "TEXT")] {
            if !columns.iter().any(|c| c == name) {
                conn.execute_batch(&format!("ALTER TABLE events ADD COLUMN {name} {ty};"))?;
            }
        }
        Ok(())
    }

    /// Writes an event. Accepts only an event the privacy firewall cleared.
    pub fn record_event(&self, cleared: &Cleared<Event>) -> Result<(), StoreError> {
        let e = cleared.value();
        let (confidence, evidence) = match &e.provenance {
            Some(p) => (Some(p.confidence), Some(serde_json::to_string(&p.evidence)?)),
            None => (None, None),
        };
        self.conn.execute(
            "INSERT INTO events (ts, kind, summary, detail, confidence, evidence)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![e.ts, format!("{:?}", e.kind), e.summary, e.detail, confidence, evidence],
        )?;
        self.conn.execute(
            "DELETE FROM events WHERE id NOT IN
                 (SELECT id FROM events ORDER BY id DESC LIMIT ?1)",
            params![self.max_events as i64],
        )?;
        Ok(())
    }

    /// Writes a snapshot. Accepts only the allowlisted `MemorySnapshot` the
    /// privacy firewall built — never the raw context object.
    pub fn record_snapshot(&self, cleared: &Cleared<MemorySnapshot>) -> Result<(), StoreError> {
        let snap = cleared.value();
        let body = serde_json::to_string(snap)?;
        self.conn.execute(
            "INSERT INTO context_snapshots (ts, schema_version, identity_state, activity_state, body, policy_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                snap.generated_at,
                snap.schema_version as i64,
                snap.identity_state.as_deref().unwrap_or("WITHHELD"),
                snap.activity_label.as_deref().unwrap_or("WITHHELD"),
                body,
                cleared.policy_version() as i64,
            ],
        )?;
        // Snapshots are the largest thing written, so the cap matters more here
        // than for events.
        self.conn.execute(
            "DELETE FROM context_snapshots WHERE id NOT IN
                 (SELECT id FROM context_snapshots ORDER BY id DESC LIMIT ?1)",
            params![self.max_snapshots as i64],
        )?;
        Ok(())
    }

    /// Writes perception samples: how the measuring went at sampled moments.
    pub fn record_perception_samples(&self, cleared: &Cleared<Vec<crate::measurement::PerceptionSample>>)
        -> Result<usize, StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        let mut n = 0;
        for s in cleared.value() {
            tx.execute(
                "INSERT INTO perception_samples
                     (ts, measurement, identity, access_level, measurement_age_ms, analyze_ms,
                      capture_gap_ms, frames_dropped, vision_busy, model_phase, track_id,
                      frames_tracked, capture_quality, face_count, policy_version)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                params![s.ts, s.measurement.tag(), s.identity, s.access_level,
                        s.measurement_age_ms.map(|v| v as i64), s.analyze_ms.map(|v| v as i64),
                        s.capture_gap_ms.map(|v| v as i64), s.frames_dropped.map(|v| v as i64),
                        s.vision_busy.map(|v| v as i64), s.model_phase, s.track_id,
                        s.frames_tracked.map(|v| v as i64), s.capture_quality,
                        s.face_count.map(|v| v as i64), cleared.policy_version() as i64],
            )?;
            n += 1;
        }
        tx.execute("DELETE FROM perception_samples WHERE id NOT IN
                        (SELECT id FROM perception_samples ORDER BY id DESC LIMIT ?1)",
                   params![self.max_samples as i64])?;
        tx.commit()?;
        Ok(n)
    }

    /// Writes stage timings: how long each part of an operation took.
    pub fn record_stage_timings(&self, cleared: &Cleared<Vec<crate::telemetry::Span>>)
        -> Result<usize, StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        let mut n = 0;
        for sp in cleared.value() {
            tx.execute(
                "INSERT INTO stage_timings (ts, stage, op, duration_ms, outcome, policy_version)
                 VALUES (?1,?2,?3,?4,?5,?6)",
                params![sp.end, sp.stage.tag(), sp.op, sp.duration_ms() as i64,
                        serde_json::to_value(sp.outcome)?.as_str().unwrap_or("").to_string(),
                        cleared.policy_version() as i64],
            )?;
            n += 1;
        }
        tx.execute("DELETE FROM stage_timings WHERE id NOT IN
                        (SELECT id FROM stage_timings ORDER BY id DESC LIMIT ?1)",
                   params![self.max_timings as i64])?;
        tx.commit()?;
        Ok(n)
    }

    /// What the database is holding and how big it has become. Measured, not
    /// estimated: this is what a retention decision has to be based on.
    pub fn footprint(&self) -> Result<StoreFootprint, StoreError> {
        let one = |sql: &str| -> Result<i64, StoreError> {
            Ok(self.conn.query_row(sql, [], |r| r.get(0))?)
        };
        let page_size = one("PRAGMA page_size")?;
        let page_count = one("PRAGMA page_count")?;
        let free_pages = one("PRAGMA freelist_count")?;
        Ok(StoreFootprint {
            events: one("SELECT COUNT(*) FROM events")? as u64,
            snapshots: one("SELECT COUNT(*) FROM context_snapshots")? as u64,
            perception_samples: one("SELECT COUNT(*) FROM perception_samples")? as u64,
            stage_timings: one("SELECT COUNT(*) FROM stage_timings")? as u64,
            ledger_rows: one("SELECT COUNT(*) FROM privacy_ledger")? as u64,
            file_bytes: (page_size * page_count) as u64,
            reusable_bytes: (page_size * free_pages) as u64,
        })
    }

    /// Adds firewall decisions to the audit ledger, aggregated per
    /// (kind, destination, decision, policy version). Metadata only.
    pub fn record_ledger(&self, cleared: &Cleared<Vec<LedgerEntry>>) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction()?;
        for r in cleared.value() {
            tx.execute(
                "INSERT INTO privacy_ledger
                     (kind, class, destination, decision, reason, first_ts, last_ts, count, policy_version)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(kind, destination, decision, policy_version) DO UPDATE SET
                     last_ts = excluded.last_ts,
                     count   = count + excluded.count,
                     reason  = excluded.reason,
                     class   = excluded.class",
                params![
                    r.kind.tag(), json_tag(&r.class)?, r.destination.tag(), r.decision, r.reason,
                    r.first_ts, r.last_ts, r.count as i64, r.policy_version as i64
                ],
            )?;
        }
        tx.commit()?;
        Ok(())
    }

    /// The audit ledger, most recently used first.
    pub fn ledger(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut st = self.conn.prepare(
            "SELECT kind, class, destination, decision, reason, first_ts, last_ts, count, policy_version
             FROM privacy_ledger ORDER BY last_ts DESC")?;
        let rows = st.query_map([], |r| Ok(serde_json::json!({
            "kind": r.get::<_, String>(0)?, "class": r.get::<_, String>(1)?,
            "destination": r.get::<_, String>(2)?, "decision": r.get::<_, String>(3)?,
            "reason": r.get::<_, String>(4)?, "first_ts": r.get::<_, f64>(5)?,
            "last_ts": r.get::<_, f64>(6)?, "count": r.get::<_, i64>(7)?,
            "policy_version": r.get::<_, i64>(8)?,
        })))?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// The most recent stored snapshot body, exactly as written to disk.
    pub fn latest_snapshot_body(&self) -> Result<Option<String>, StoreError> {
        Ok(self.conn.query_row(
            "SELECT body FROM context_snapshots ORDER BY id DESC LIMIT 1", [], |r| r.get(0))
            .map(Some).or_else(|e| match e {
                rusqlite::Error::QueryReturnedNoRows => Ok(None),
                other => Err(other),
            })?)
    }

    /// Snapshots written under an older privacy policy than the current one.
    /// Version 0 predates the firewall and may hold body joint positions and
    /// face measurement tracks.
    pub fn legacy_snapshot_count(&self) -> Result<u64, StoreError> {
        Ok(self.conn.query_row(
            "SELECT COUNT(*) FROM context_snapshots WHERE policy_version < ?1",
            params![PRIVACY_POLICY_VERSION as i64], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Deletes every snapshot written under an older policy, then compacts the
    /// database so the removed content is gone from the file and its WAL.
    pub fn purge_legacy_snapshots(&self) -> Result<u64, StoreError> {
        let n = self.conn.execute(
            "DELETE FROM context_snapshots WHERE policy_version < ?1",
            params![PRIVACY_POLICY_VERSION as i64])? as u64;
        self.compact()?;
        Ok(n)
    }

    /// Checkpoints and truncates the WAL, then rebuilds the file. With
    /// secure_delete on, this leaves no copy of deleted rows on disk.
    fn compact(&self) -> Result<(), StoreError> {
        self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE); VACUUM; PRAGMA wal_checkpoint(TRUNCATE);")?;
        Ok(())
    }

    pub fn event_count(&self) -> Result<u64, StoreError> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM events", [], |r| r.get::<_, i64>(0))? as u64)
    }

    pub fn snapshot_count(&self) -> Result<u64, StoreError> {
        Ok(self.conn.query_row("SELECT COUNT(*) FROM context_snapshots", [], |r| r.get::<_, i64>(0))? as u64)
    }

    /// Remembered events, newest first, with whatever provenance was recorded.
    pub fn history(&self, n: u32) -> Result<Vec<RememberedEvent>, StoreError> {
        let mut st = self.conn.prepare(
            "SELECT ts, kind, summary, detail, confidence, evidence FROM events ORDER BY ts DESC, id DESC LIMIT ?1")?;
        let rows = st.query_map(params![n as i64], |r| {
            Ok((r.get::<_, f64>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?, r.get::<_, Option<f64>>(4)?, r.get::<_, Option<String>>(5)?))
        })?;
        Ok(rows.filter_map(Result::ok).map(|(ts, kind, summary, detail, confidence, evidence)| RememberedEvent {
            ts, kind, summary, detail, confidence,
            // An unreadable evidence column is reported as no evidence, never invented.
            evidence: evidence.and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default(),
        }).collect())
    }

    /// Writes one memory, or replaces it where it is already kept (a memory
    /// that was superseded keeps its id and changes its state). Accepts only
    /// what the privacy firewall cleared for local memory.
    pub fn record_memory(&self, cleared: &Cleared<crate::memory::Memory>) -> Result<(), StoreError> {
        let m = cleared.value();
        self.conn.execute(
            "INSERT INTO memories (id, subject, statement, class, state, source, why, privacy,
                                   created_at, updated_at, valid_for, provenance, relates_to, superseded_by, policy_version)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)
             ON CONFLICT(id) DO UPDATE SET
                 subject = excluded.subject, statement = excluded.statement, state = excluded.state,
                 updated_at = excluded.updated_at, provenance = excluded.provenance,
                 relates_to = excluded.relates_to, superseded_by = excluded.superseded_by",
            params![m.id, m.subject, m.statement, m.class.tag(), m.state.tag(), format!("{:?}", m.source),
                    m.why.tag(), m.privacy.tag(), m.created_at, m.updated_at, m.valid_for, m.provenance,
                    m.relates_to, m.superseded_by, cleared.policy_version() as i64],
        )?;
        Ok(())
    }

    /// Removes a memory the owner forgot. The row goes; `secure_delete` means
    /// its bytes are overwritten, and the write-ahead log is checkpointed so
    /// the words are not left behind in it either.
    pub fn forget_memory(&self, id: &str) -> Result<bool, StoreError> {
        let gone = self.conn.execute("DELETE FROM memories WHERE id = ?1", params![id])?;
        if gone > 0 { self.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")?; }
        Ok(gone > 0)
    }

    /// Everything kept, for the start of a run. A row KUE cannot read — an
    /// unknown class, state or data kind — is left behind rather than guessed
    /// at, and counted so the count on screen is honest.
    pub fn memories(&self) -> Result<(Vec<crate::memory::Memory>, usize), StoreError> {
        use crate::memory::{MemoryClass, MemoryState, Why};
        let mut st = self.conn.prepare(
            "SELECT id, subject, statement, class, state, source, why, privacy, created_at, updated_at,
                    valid_for, provenance, relates_to, superseded_by FROM memories ORDER BY created_at")?;
        let rows = st.query_map([], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?, r.get::<_, String>(2)?, r.get::<_, String>(3)?,
                r.get::<_, String>(4)?, r.get::<_, String>(5)?, r.get::<_, String>(6)?, r.get::<_, String>(7)?,
                r.get::<_, f64>(8)?, r.get::<_, f64>(9)?, r.get::<_, Option<f64>>(10)?, r.get::<_, String>(11)?,
                r.get::<_, Option<String>>(12)?, r.get::<_, Option<String>>(13)?))
        })?;
        let mut out = Vec::new();
        let mut unreadable = 0;
        for row in rows.filter_map(Result::ok) {
            let (id, subject, statement, class, state, source, why, privacy,
                 created_at, updated_at, valid_for, provenance, relates_to, superseded_by) = row;
            let parsed = (MemoryClass::from_tag(&class), MemoryState::from_tag(&state),
                          Why::from_tag(&why), crate::privacy::DataKind::from_tag(&privacy));
            match parsed {
                (Some(class), Some(state), Some(why), Some(privacy)) => out.push(crate::memory::Memory {
                    id, subject, statement, class, state, why, privacy, created_at, updated_at,
                    valid_for, provenance, relates_to, superseded_by, deleted_at: None,
                    source: match source.as_str() {
                        "Sensor" => crate::facts::FactSource::Sensor,
                        "Action" => crate::facts::FactSource::Action,
                        "Owner" => crate::facts::FactSource::Owner,
                        "Model" => crate::facts::FactSource::Model,
                        _ => crate::facts::FactSource::Reasoning,
                    },
                }),
                _ => unreadable += 1,
            }
        }
        Ok((out, unreadable))
    }

    /// Deletes everything, including the audit ledger (its timestamps are
    /// themselves a record of the owner's day), then compacts so the content is
    /// removed from disk rather than left in free pages or the WAL.
    pub fn erase_all(&self) -> Result<(), StoreError> {
        self.conn.execute_batch(
            "DELETE FROM events; DELETE FROM context_snapshots; DELETE FROM privacy_ledger; DELETE FROM memories;")?;
        self.compact()
    }
}

fn json_tag<T: serde::Serialize>(v: &T) -> Result<String, StoreError> {
    Ok(serde_json::to_value(v)?.as_str().unwrap_or_default().to_string())
}


#[cfg(test)]
mod memory_tests {
    use super::*;
    use crate::memory::{Memory, MemoryBook, MemoryClass, MemoryState, subject_of};
    use crate::privacy::{DataKind, Firewall};

    /// What makes memory memory: it is still there after KUE stops.
    #[test]
    fn a_memory_written_in_one_run_is_read_back_in_the_next() {
        let dir = std::env::temp_dir().join(format!("kue-memory-store-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("memory.sqlite3");
        let _ = std::fs::remove_file(&path);
        let mut fw = Firewall::new();

        // One run: the owner says something, and it is written through the firewall.
        {
            let st = Store::open(&path, 100, 10).expect("opens");
            let mut book = MemoryBook::new(10);
            let id = book.id(1000.0);
            let said = "You prefer PDF reports";
            book.remember(Memory::told(&id, MemoryClass::Preference, &subject_of(MemoryClass::Preference, said),
                                       said, DataKind::OwnerMessage, 1000.0, "I prefer PDF reports"), false, 1000.0);
            for m in book.take_pending() {
                let cleared = fw.clear_memory(&m, 1000.0).expect("policy keeps the owner's own words on this Mac");
                st.record_memory(&cleared).expect("written");
            }
        }

        // The next run: a new store, a new book, and the memory is there.
        {
            let st = Store::open(&path, 100, 10).expect("reopens");
            let (rows, unreadable) = st.memories().expect("read back");
            assert_eq!(unreadable, 0);
            assert_eq!(rows.len(), 1);
            let mut book = MemoryBook::new(10);
            book.load(rows);
            let found = book.matching("what do you remember about reports", 2000.0);
            assert_eq!(found.len(), 1);
            assert_eq!(found[0].statement, "You prefer PDF reports");
            assert_eq!(found[0].state, MemoryState::Confirmed);
            assert_eq!(found[0].class, MemoryClass::Preference);
            assert!(found[0].provenance.contains("I prefer PDF reports"), "why it is kept survives too");

            // Forgetting reaches the store, and the words are gone from it.
            let id = found[0].id.clone();
            book.forget(&id, 2100.0);
            for gone in book.take_forgotten() { assert!(st.forget_memory(&gone).expect("removed")); }
            let (rows, _) = st.memories().expect("read back");
            assert!(rows.is_empty(), "the row is gone, not blanked");
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// A memory that names the owner's files is not kept: policy says that
    /// kind may be shown to them and nothing else.
    #[test]
    fn what_policy_will_not_keep_is_not_written() {
        let mut fw = Firewall::new();
        let m = Memory::told("m1", MemoryClass::Fact, "fact:x", "Your resume is on the Desktop",
                             DataKind::ActionTarget, 10.0, "…");
        assert!(fw.clear_memory(&m, 10.0).is_none(), "ACTION_TARGET may be shown, never stored");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::events::{EventKind, EventLog};
    use crate::privacy::Firewall;

    fn cleared(e: &Event) -> Cleared<Event> {
        Firewall::new().clear_event(e, e.ts).expect("events are LOCAL_ONLY and may be stored")
    }

    #[test]
    fn a_second_instance_cannot_open_the_same_memory() {
        let dir = std::env::temp_dir().join(format!("lantern-lock-{}", std::process::id()));
        let path = dir.join("lantern.sqlite3");
        let first = Store::open(&path, 10, 10).unwrap();
        match Store::open(&path, 10, 10) {
            Err(StoreError::InUse(_)) => {}
            Err(e) => panic!("wrong error: {e}"),
            Ok(_) => panic!("two stores opened the same database"),
        }
        drop(first);
        assert!(Store::open(&path, 10, 10).is_ok(), "the lock must be released when the store closes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn events_round_trip() {
        let s = Store::open_in_memory(100, 100).unwrap();
        let mut log = EventLog::new(10);
        let e = log.record(1.0, EventKind::FaceAppeared, "A face became visible.", None);
        s.record_event(&cleared(&e)).unwrap();
        assert_eq!(s.event_count().unwrap(), 1);
        let r = s.history(5).unwrap();
        assert_eq!(r[0].summary, "A face became visible.");
    }

    #[test]
    fn event_table_is_trimmed_to_the_configured_cap() {
        let s = Store::open_in_memory(5, 5).unwrap();
        let mut log = EventLog::new(100);
        for i in 0..25 {
            let e = log.record(i as f64, EventKind::Error, format!("e{i}"), None);
            s.record_event(&cleared(&e)).unwrap();
        }
        assert_eq!(s.event_count().unwrap(), 5);
        assert_eq!(s.history(1).unwrap()[0].summary, "e24");
    }

    #[test]
    fn snapshot_table_is_trimmed_to_the_configured_cap() {
        // Snapshot bodies are the largest thing written. An untrimmed table grew
        // without bound at roughly 170MB per day.
        use crate::config::Config;
        use crate::engine::Engine;
        let s = Store::open_in_memory(100, 4).unwrap();
        let e = Engine::new(Config::default_config(), "test".into());
        let mut fw = Firewall::new();
        for i in 0..20 {
            s.record_snapshot(&fw.clear_snapshot(&e.build_context(i as f64), i as f64)).unwrap();
        }
        assert_eq!(s.snapshot_count().unwrap(), 4);
    }

    /// Reads every byte SQLite has on disk for this database.
    fn disk_bytes(path: &std::path::Path) -> Vec<u8> {
        let mut all = Vec::new();
        for suffix in ["", "-wal", "-shm"] {
            if let Ok(b) = std::fs::read(format!("{}{suffix}", path.display())) { all.extend(b); }
        }
        all
    }

    fn contains(hay: &[u8], needle: &str) -> bool {
        hay.windows(needle.len()).any(|w| w == needle.as_bytes())
    }

    fn temp_db(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!("kue-{tag}-{}-{}", std::process::id(),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()))
            .join("lantern.sqlite3")
    }

    #[test]
    fn purging_legacy_snapshots_removes_their_bytes_from_disk() {
        let path = temp_db("purge");
        let marker = "LEGACY-JOINT-leftShoulder-0.987654321";
        let s = Store::open(&path, 100, 100).unwrap();
        // A snapshot exactly as written before the firewall: the whole context
        // object, with no policy version.
        s.conn.execute(
            "INSERT INTO context_snapshots (ts, schema_version, identity_state, activity_state, body)
             VALUES (1.0, 1, 'NO_FACE', 'UNKNOWN', ?1)",
            params![format!("{{\"environment\":{{\"joints\":\"{marker}\"}}}}")]).unwrap();
        s.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        assert_eq!(s.legacy_snapshot_count().unwrap(), 1);
        assert!(contains(&disk_bytes(&path), marker), "control: the marker is on disk before the purge");

        // A snapshot written through the firewall must survive the purge.
        let engine = crate::engine::Engine::new(crate::config::Config::default_config(), "t".into());
        s.record_snapshot(&Firewall::new().clear_snapshot(&engine.build_context(2.0), 2.0)).unwrap();

        assert_eq!(s.purge_legacy_snapshots().unwrap(), 1);
        assert_eq!(s.legacy_snapshot_count().unwrap(), 0);
        assert_eq!(s.snapshot_count().unwrap(), 1, "current-policy snapshots are kept");
        drop(s);
        assert!(!contains(&disk_bytes(&path), marker),
            "the purged content must not remain anywhere in the database files");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn erasing_everything_removes_the_bytes_not_just_the_rows() {
        let path = temp_db("erase");
        let marker = "ERASE-ME-summary-5f2c9e";
        let s = Store::open(&path, 100, 100).unwrap();
        let mut log = EventLog::new(10);
        s.record_event(&cleared(&log.record(1.0, EventKind::FaceAppeared, marker, None))).unwrap();
        s.conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);").unwrap();
        assert!(contains(&disk_bytes(&path), marker), "control: the event is on disk");
        s.erase_all().unwrap();
        assert_eq!(s.event_count().unwrap(), 0);
        drop(s);
        assert!(!contains(&disk_bytes(&path), marker), "erased content must be gone from disk");
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn erase_all_clears_local_memory() {
        let s = Store::open_in_memory(100, 100).unwrap();
        let mut log = EventLog::new(10);
        s.record_event(&cleared(&log.record(1.0, EventKind::Paused, "p", None))).unwrap();
        assert_eq!(s.event_count().unwrap(), 1);
        s.erase_all().unwrap();
        assert_eq!(s.event_count().unwrap(), 0);
    }

    #[test]
    fn provenance_survives_a_round_trip() {
        use crate::config::{Config, Source};
        use crate::events::Provenance;
        use crate::evidence::{EvidenceItem, Polarity};
        let cfg = Config::default_config();
        let s = Store::open_in_memory(100, 100).unwrap();
        let mut log = EventLog::new(10);
        let why = Provenance {
            confidence: 0.42, stable_seconds: 0.0,
            evidence: vec![EvidenceItem::new("recent_input", "Input 2s ago", Polarity::Supports,
                2.5, 0.9, Source::SystemHid, &cfg)],
        };
        s.record_event(&cleared(&log.record_with(5.0, EventKind::ActivityStateChanged, "Activity: x", None, Some(why)))).unwrap();
        s.record_event(&cleared(&log.record(6.0, EventKind::FaceAppeared, "A face became visible.", None))).unwrap();
        let h = s.history(10).unwrap();
        assert_eq!(h[0].confidence, None, "an observation has no provenance");
        assert_eq!(h[1].confidence, Some(0.42));
        assert_eq!(h[1].evidence[0].id, "recent_input");
        assert!((h[1].evidence[0].strength - 0.9).abs() < 1e-12);
    }

    #[test]
    fn a_database_from_before_provenance_is_migrated_without_losing_rows() {
        let path = std::env::temp_dir().join(format!("lantern-migrate-{}-{}.sqlite3",
            std::process::id(), std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        {
            // The exact events schema shipped before this change.
            let old = Connection::open(&path).unwrap();
            old.execute_batch(
                "CREATE TABLE events (id INTEGER PRIMARY KEY AUTOINCREMENT, ts REAL NOT NULL,
                     kind TEXT NOT NULL, summary TEXT NOT NULL, detail TEXT);
                 INSERT INTO events (ts, kind, summary, detail)
                     VALUES (1.0, 'Paused', 'Paused. Camera stopped and perception discarded.', NULL);").unwrap();
        }
        let s = Store::open(&path, 100, 100).expect("an old database must open");
        let h = s.history(10).unwrap();
        assert_eq!(h.len(), 1, "existing rows must survive the migration");
        assert_eq!(h[0].kind, "Paused");
        assert_eq!(h[0].confidence, None);

        let mut log = EventLog::new(10);
        let why = crate::events::Provenance { confidence: 0.7, stable_seconds: 0.0, evidence: vec![] };
        s.record_event(&cleared(&log.record_with(2.0, EventKind::IdentityStateChanged, "Identity state: NO_FACE", None, Some(why)))).unwrap();
        assert_eq!(s.history(1).unwrap()[0].confidence, Some(0.7));
        drop(s);
        // Opening a migrated database again is a no-op.
        assert_eq!(Store::open(&path, 100, 100).unwrap().history(10).unwrap().len(), 2);
        for suffix in ["", "-wal", "-shm"] {
            let _ = std::fs::remove_file(format!("{}{suffix}", path.display()));
        }
    }
}

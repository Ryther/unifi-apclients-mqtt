//! Persistent, AP-scoped observation history for whitelist eligibility.

use std::{
    collections::{HashMap, HashSet},
    fs::{self, OpenOptions},
    io,
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::Path,
};

use rusqlite::{Connection, Transaction, params};

use crate::mapper::{ApSnapshot, ClientSnapshot};

pub const CONTINUOUS_SECONDS: i64 = 24 * 60 * 60;
pub const CUMULATIVE_SECONDS: i64 = 5 * 60 * 60;
pub const WINDOW_SECONDS: i64 = 48 * 60 * 60;

const SCHEMA_VERSION: i64 = 1;

#[derive(Default)]
struct ClientHistory {
    intervals: Vec<Interval>,
    active_since: Option<i64>,
    last_seen: Option<i64>,
}

#[derive(Clone)]
struct Interval {
    start: i64,
    end: i64,
    observed_absence_after: bool,
}

pub struct PresenceHistory {
    connection: Connection,
    max_gap_seconds: i64,
}

impl PresenceHistory {
    pub fn open(path: &Path, now_unix: i64, max_gap_seconds: i64) -> io::Result<Self> {
        if max_gap_seconds <= 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "presence observation gap must be positive",
            ));
        }
        let parent = path.parent().ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "history path has no parent")
        })?;
        fs::create_dir_all(parent)?;
        OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;
        let mut connection = sql(Connection::open(path))?;
        fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        sql(connection.execute_batch("PRAGMA synchronous = FULL; PRAGMA foreign_keys = ON;"))?;
        let version: i64 = sql(connection.query_row("PRAGMA user_version", [], |row| row.get(0)))?;
        if version != 0 && version != SCHEMA_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "unsupported presence history schema",
            ));
        }
        let transaction = sql(connection.transaction())?;
        if version == 0 {
            sql(transaction.execute_batch(
                "CREATE TABLE client_presence (
                    ap_mac TEXT NOT NULL,
                    client_mac TEXT NOT NULL,
                    active_since INTEGER,
                    last_seen INTEGER,
                    PRIMARY KEY (ap_mac, client_mac)
                );
                CREATE TABLE presence_intervals (
                    ap_mac TEXT NOT NULL,
                    client_mac TEXT NOT NULL,
                    started_at INTEGER NOT NULL,
                    ended_at INTEGER NOT NULL,
                    observed_absence_after INTEGER NOT NULL,
                    FOREIGN KEY (ap_mac, client_mac)
                        REFERENCES client_presence (ap_mac, client_mac) ON DELETE CASCADE
                );
                CREATE INDEX presence_intervals_end ON presence_intervals (ended_at);
                CREATE INDEX presence_intervals_client ON presence_intervals (ap_mac, client_mac);
                PRAGMA user_version = 1;",
            ))?;
        }
        // A restart ends every open observation at its last confirmed poll.
        sql(transaction.execute(
            "INSERT INTO presence_intervals
                (ap_mac, client_mac, started_at, ended_at, observed_absence_after)
             SELECT ap_mac, client_mac, active_since, last_seen, 0
             FROM client_presence
             WHERE active_since IS NOT NULL AND last_seen > active_since",
            [],
        ))?;
        sql(transaction.execute(
            "UPDATE client_presence SET active_since = NULL, last_seen = NULL
             WHERE active_since IS NOT NULL",
            [],
        ))?;
        prune(&transaction, now_unix)?;
        sql(transaction.commit())?;
        Ok(Self {
            connection,
            max_gap_seconds,
        })
    }

    pub fn observe_ap(
        &mut self,
        snapshot: &ApSnapshot,
        now_unix: i64,
    ) -> io::Result<Vec<ClientSnapshot>> {
        let ap_mac = snapshot.ap_mac.to_ascii_lowercase();
        let present: HashSet<String> = snapshot
            .clients
            .iter()
            .map(|client| client.mac.to_ascii_lowercase())
            .collect();
        let transaction = sql(self.connection.transaction())?;
        let mut histories = load_ap(&transaction, &ap_mac)?;

        for (mac, record) in &mut histories {
            if !present.contains(mac) && record.active_since.is_some() {
                if let Some(interval) = record.close_active(true) {
                    insert_interval(&transaction, &ap_mac, mac, &interval)?;
                }
                save_client(&transaction, &ap_mac, mac, record)?;
            } else if let Some(last_seen) = record.last_seen
                && (now_unix < last_seen
                    || now_unix.saturating_sub(last_seen) > self.max_gap_seconds)
                && let Some(interval) = record.close_active(false)
            {
                insert_interval(&transaction, &ap_mac, mac, &interval)?;
            }
        }
        for mac in &present {
            let record = histories.entry(mac.clone()).or_default();
            if record.active_since.is_none() {
                record.active_since = Some(now_unix);
            }
            record.last_seen = Some(now_unix);
            save_client(&transaction, &ap_mac, mac, record)?;
        }
        prune(&transaction, now_unix)?;
        let eligible = snapshot
            .clients
            .iter()
            .filter(|client| {
                histories
                    .get(&client.mac.to_ascii_lowercase())
                    .is_some_and(|record| record.eligible(now_unix))
            })
            .cloned()
            .collect();
        sql(transaction.commit())?;
        Ok(eligible)
    }

    pub fn mark_unavailable(&mut self, ap_mac: &str, now_unix: i64) -> io::Result<()> {
        let ap_mac = ap_mac.to_ascii_lowercase();
        let transaction = sql(self.connection.transaction())?;
        let mut histories = load_ap(&transaction, &ap_mac)?;
        for (mac, record) in &mut histories {
            if record.active_since.is_some() {
                if let Some(interval) = record.close_active(false) {
                    insert_interval(&transaction, &ap_mac, mac, &interval)?;
                }
                save_client(&transaction, &ap_mac, mac, record)?;
            }
        }
        prune(&transaction, now_unix)?;
        sql(transaction.commit())
    }
}

fn load_ap(
    transaction: &Transaction<'_>,
    ap_mac: &str,
) -> io::Result<HashMap<String, ClientHistory>> {
    let mut histories = HashMap::new();
    let mut statement = sql(transaction.prepare(
        "SELECT client_mac, active_since, last_seen FROM client_presence WHERE ap_mac = ?1",
    ))?;
    let rows = sql(statement.query_map([ap_mac], |row| {
        Ok((
            row.get::<_, String>(0)?,
            row.get::<_, Option<i64>>(1)?,
            row.get::<_, Option<i64>>(2)?,
        ))
    }))?;
    for row in rows {
        let (mac, active_since, last_seen) = sql(row)?;
        histories.insert(
            mac,
            ClientHistory {
                intervals: Vec::new(),
                active_since,
                last_seen,
            },
        );
    }
    let mut statement = sql(transaction.prepare(
        "SELECT client_mac, started_at, ended_at, observed_absence_after
         FROM presence_intervals WHERE ap_mac = ?1",
    ))?;
    let rows = sql(statement.query_map([ap_mac], |row| {
        Ok((
            row.get::<_, String>(0)?,
            Interval {
                start: row.get(1)?,
                end: row.get(2)?,
                observed_absence_after: row.get(3)?,
            },
        ))
    }))?;
    for row in rows {
        let (mac, interval) = sql(row)?;
        if let Some(record) = histories.get_mut(&mac) {
            record.intervals.push(interval);
        }
    }
    Ok(histories)
}

fn save_client(
    transaction: &Transaction<'_>,
    ap_mac: &str,
    mac: &str,
    record: &ClientHistory,
) -> io::Result<()> {
    sql(transaction.execute(
        "INSERT INTO client_presence (ap_mac, client_mac, active_since, last_seen)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT (ap_mac, client_mac) DO UPDATE SET
             active_since = excluded.active_since, last_seen = excluded.last_seen",
        params![ap_mac, mac, record.active_since, record.last_seen],
    ))?;
    Ok(())
}

fn insert_interval(
    transaction: &Transaction<'_>,
    ap_mac: &str,
    mac: &str,
    interval: &Interval,
) -> io::Result<()> {
    sql(transaction.execute(
        "INSERT INTO presence_intervals
            (ap_mac, client_mac, started_at, ended_at, observed_absence_after)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        params![
            ap_mac,
            mac,
            interval.start,
            interval.end,
            interval.observed_absence_after
        ],
    ))?;
    Ok(())
}

fn prune(transaction: &Transaction<'_>, now_unix: i64) -> io::Result<()> {
    let cutoff = now_unix.saturating_sub(WINDOW_SECONDS);
    sql(transaction.execute(
        "DELETE FROM presence_intervals WHERE ended_at <= ?1 OR started_at >= ended_at",
        [cutoff],
    ))?;
    sql(transaction.execute(
        "DELETE FROM client_presence WHERE active_since IS NULL AND NOT EXISTS (
            SELECT 1 FROM presence_intervals AS i
            WHERE i.ap_mac = client_presence.ap_mac AND i.client_mac = client_presence.client_mac
        )",
        [],
    ))?;
    Ok(())
}

fn sql<T>(result: rusqlite::Result<T>) -> io::Result<T> {
    result.map_err(io::Error::other)
}

impl ClientHistory {
    fn close_active(&mut self, observed_absence_after: bool) -> Option<Interval> {
        let interval = match (self.active_since, self.last_seen) {
            (Some(start), Some(end)) if end > start => Some(Interval {
                start,
                end,
                observed_absence_after,
            }),
            _ => None,
        };
        self.active_since = None;
        self.last_seen = None;
        if let Some(interval) = &interval {
            self.intervals.push(interval.clone());
        }
        interval
    }

    fn eligible(&self, now_unix: i64) -> bool {
        let cutoff = now_unix.saturating_sub(WINDOW_SECONDS);
        if self
            .active_since
            .is_some_and(|start| now_unix.saturating_sub(start) >= CONTINUOUS_SECONDS)
        {
            return true;
        }
        let completed = self.intervals.iter().fold(0_i64, |total, interval| {
            total.saturating_add(
                interval
                    .end
                    .min(now_unix)
                    .saturating_sub(interval.start.max(cutoff)),
            )
        });
        let current = self
            .active_since
            .map_or(0, |start| now_unix.saturating_sub(start.max(cutoff)));
        let observed_discontinuity = self.intervals.iter().any(|interval| {
            interval.observed_absence_after && interval.end > cutoff && interval.end < now_unix
        });
        observed_discontinuity && completed.saturating_add(current) >= CUMULATIVE_SECONDS
    }
}

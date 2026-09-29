use std::{
    fs, io,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
    time::{SystemTime, UNIX_EPOCH},
};

use unifi_apclients_mqtt::{
    mapper::{ApSnapshot, ClientSnapshot},
    presence_history::PresenceHistory,
};

const MAX_GAP_SECONDS: i64 = 3_600;
const AP_A: &str = "02:aa:00:00:01:01";
const AP_B: &str = "02:cc:00:00:01:02";
const CLIENT: &str = "02:bb:00:00:00:01";

static NEXT_TEMP_ID: AtomicU64 = AtomicU64::new(0);

struct TempHistory {
    dir: PathBuf,
}

impl TempHistory {
    fn new() -> io::Result<Self> {
        let id = NEXT_TEMP_ID.fetch_add(1, Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before Unix epoch")
            .as_nanos();
        let dir = std::env::temp_dir().join(format!(
            "unifi-presence-history-{}-{nanos}-{id}",
            std::process::id()
        ));
        fs::create_dir(&dir)?;
        Ok(Self { dir })
    }

    fn path(&self) -> PathBuf {
        self.dir.join("history.db")
    }
}

impl Drop for TempHistory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn client(mac: &str) -> ClientSnapshot {
    ClientSnapshot {
        mac: mac.to_owned(),
        name: "Test client".to_owned(),
        hostname: None,
        ip: None,
    }
}

fn snapshot(ap_mac: &str, client_mac: Option<&str>) -> ApSnapshot {
    ApSnapshot {
        available: true,
        ap_mac: ap_mac.to_owned(),
        ap_name: "Test AP".to_owned(),
        clients: client_mac.into_iter().map(client).collect(),
    }
}

fn eligible_macs(clients: Vec<ClientSnapshot>) -> Vec<String> {
    clients
        .into_iter()
        .map(|client| client.mac.to_ascii_lowercase())
        .collect()
}

#[test]
fn cumulative_presence_qualifies_at_five_hours_across_separate_sessions() -> io::Result<()> {
    let temp = TempHistory::new()?;
    let mut history = PresenceHistory::open(&temp.path(), 0, MAX_GAP_SECONDS)?;
    let present = snapshot(AP_A, Some(CLIENT));
    let absent = snapshot(AP_A, None);

    for now in [0, 3_600, 7_200] {
        assert!(history.observe_ap(&present, now)?.is_empty());
    }
    assert!(history.observe_ap(&absent, 10_800)?.is_empty());
    for now in [14_400, 18_000, 21_600] {
        assert!(history.observe_ap(&present, now)?.is_empty());
    }
    assert_eq!(
        eligible_macs(history.observe_ap(&present, 25_200)?),
        vec![CLIENT]
    );
    Ok(())
}

#[test]
fn poll_gap_over_limit_does_not_credit_elapsed_time() -> io::Result<()> {
    let temp = TempHistory::new()?;
    let mut history = PresenceHistory::open(&temp.path(), 0, MAX_GAP_SECONDS)?;
    let present = snapshot(AP_A, Some(CLIENT));

    for now in [0, 3_600, 7_200, 10_800, 14_400, 18_000] {
        assert!(history.observe_ap(&present, now)?.is_empty());
    }
    // A gap alone cannot turn two stretches of presence into observed absence.
    for now in [25_200, 28_800, 32_400] {
        assert!(history.observe_ap(&present, now)?.is_empty());
    }
    Ok(())
}

#[test]
fn reopening_preserves_cumulative_time_without_crediting_downtime() -> io::Result<()> {
    let temp = TempHistory::new()?;
    let present = snapshot(AP_A, Some(CLIENT));
    {
        let mut history = PresenceHistory::open(&temp.path(), 0, MAX_GAP_SECONDS)?;
        for now in [0, 3_600, 7_200] {
            assert!(history.observe_ap(&present, now)?.is_empty());
        }
        assert!(
            history
                .observe_ap(&snapshot(AP_A, None), 10_800)?
                .is_empty()
        );
        for now in [14_400, 18_000] {
            assert!(history.observe_ap(&present, now)?.is_empty());
        }
    }

    let mut history = PresenceHistory::open(&temp.path(), 21_600, MAX_GAP_SECONDS)?;
    for now in [21_600, 25_200] {
        assert!(history.observe_ap(&present, now)?.is_empty());
    }
    assert_eq!(
        eligible_macs(history.observe_ap(&present, 28_800)?),
        vec![CLIENT]
    );
    Ok(())
}

#[test]
fn ap_unavailability_does_not_prove_intermittent_absence() -> io::Result<()> {
    let temp = TempHistory::new()?;
    let mut history = PresenceHistory::open(&temp.path(), 0, MAX_GAP_SECONDS)?;
    let present = snapshot(AP_A, Some(CLIENT));

    for now in [0, 3_600, 7_200] {
        assert!(history.observe_ap(&present, now)?.is_empty());
    }
    history.mark_unavailable(AP_A, 10_800)?;
    for now in [14_400, 18_000, 21_600, 25_200] {
        assert!(history.observe_ap(&present, now)?.is_empty());
    }
    Ok(())
}

#[test]
fn continuous_presence_needs_a_full_day() -> io::Result<()> {
    let temp = TempHistory::new()?;
    let mut history = PresenceHistory::open(&temp.path(), 0, MAX_GAP_SECONDS)?;
    let present = snapshot(AP_A, Some(CLIENT));

    for hour in 0..24 {
        assert!(
            history
                .observe_ap(&present, i64::from(hour) * 3_600)?
                .is_empty()
        );
    }
    assert_eq!(
        eligible_macs(history.observe_ap(&present, 86_400)?),
        vec![CLIENT]
    );
    Ok(())
}

#[test]
fn observed_absence_before_a_day_does_not_emit_stale_client() -> io::Result<()> {
    let temp = TempHistory::new()?;
    let mut history = PresenceHistory::open(&temp.path(), 0, MAX_GAP_SECONDS)?;
    let present = snapshot(AP_A, Some(CLIENT));

    for hour in 0..24 {
        assert!(
            history
                .observe_ap(&present, i64::from(hour) * 3_600)?
                .is_empty()
        );
    }
    assert!(
        history
            .observe_ap(&snapshot(AP_A, None), 86_400)?
            .is_empty()
    );
    Ok(())
}

#[test]
fn cumulative_presence_expires_outside_rolling_two_days() -> io::Result<()> {
    let temp = TempHistory::new()?;
    let mut history = PresenceHistory::open(&temp.path(), 0, MAX_GAP_SECONDS)?;
    let present = snapshot(AP_A, Some(CLIENT));

    for now in [0, 3_600, 7_200] {
        assert!(history.observe_ap(&present, now)?.is_empty());
    }
    assert!(
        history
            .observe_ap(&snapshot(AP_A, None), 10_800)?
            .is_empty()
    );
    for now in [14_400, 18_000, 21_600] {
        assert!(history.observe_ap(&present, now)?.is_empty());
    }
    assert_eq!(
        eligible_macs(history.observe_ap(&present, 25_200)?),
        vec![CLIENT]
    );
    assert!(history.observe_ap(&present, 176_400)?.is_empty());
    Ok(())
}

#[test]
fn mac_case_is_ignored_but_ap_histories_stay_separate() -> io::Result<()> {
    let temp = TempHistory::new()?;
    let mut history = PresenceHistory::open(&temp.path(), 0, MAX_GAP_SECONDS)?;
    let upper_ap = AP_A.to_ascii_uppercase();
    let upper_client = CLIENT.to_ascii_uppercase();

    for (now, ap, mac) in [
        (0, AP_A, CLIENT),
        (3_600, upper_ap.as_str(), upper_client.as_str()),
        (7_200, AP_A, CLIENT),
    ] {
        assert!(
            history
                .observe_ap(&snapshot(ap, Some(mac)), now)?
                .is_empty()
        );
    }
    assert!(
        history
            .observe_ap(&snapshot(&upper_ap, None), 10_800)?
            .is_empty()
    );
    for (now, ap, mac) in [
        (14_400, upper_ap.as_str(), upper_client.as_str()),
        (18_000, AP_A, CLIENT),
        (21_600, upper_ap.as_str(), upper_client.as_str()),
    ] {
        assert!(
            history
                .observe_ap(&snapshot(ap, Some(mac)), now)?
                .is_empty()
        );
    }
    assert_eq!(
        eligible_macs(history.observe_ap(&snapshot(&upper_ap, Some(&upper_client)), 25_200)?),
        vec![CLIENT]
    );
    assert!(
        history
            .observe_ap(&snapshot(AP_B, Some(CLIENT)), 25_200)?
            .is_empty()
    );
    Ok(())
}

#[test]
fn absent_clients_are_not_returned_even_with_qualifying_history() -> io::Result<()> {
    let temp = TempHistory::new()?;
    let mut history = PresenceHistory::open(&temp.path(), 0, MAX_GAP_SECONDS)?;
    let present = snapshot(AP_A, Some(CLIENT));
    for now in [0, 3_600, 7_200] {
        history.observe_ap(&present, now)?;
    }
    history.observe_ap(&snapshot(AP_A, None), 10_800)?;
    for now in [14_400, 18_000, 21_600, 25_200] {
        history.observe_ap(&present, now)?;
    }

    assert!(
        history
            .observe_ap(&snapshot(AP_A, None), 28_800)?
            .is_empty()
    );
    Ok(())
}

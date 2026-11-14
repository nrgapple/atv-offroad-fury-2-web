use super::*;

struct Counter(u8);
impl Entropy for Counter {
    fn fill(&mut self, out: &mut [u8]) {
        for b in out {
            self.0 = self.0.wrapping_add(29);
            *b = self.0;
        }
    }
}

const NOW: u64 = 1_763_130_000_000;
const ROOM: atv_net::RoomCode = *b"ABCD2345";

fn secret(n: u8) -> String {
    format!("{n:02x}").repeat(16)
}

/// Registers rider `n`: (player id, friend code).
fn join(p: &Players, n: u8, name: &str) -> (String, String) {
    let reply = p
        .register(&secret(n), name, NOW, &mut Counter(n))
        .expect("register");
    let pid = p.verify(reply["token"].as_str().unwrap(), NOW).unwrap();
    (pid, reply["code"].as_str().unwrap().to_string())
}

fn players() -> Players {
    Players::with_key(&[3; 32])
}

/// The reference `viewer` holds for the rider named `name` in `list`.
fn reference(p: &Players, viewer: &str, list: &str, name: &str) -> String {
    let lists = p.friends(viewer, NOW).unwrap();
    lists[list]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["name"] == name)
        .unwrap_or_else(|| panic!("{name} in {list}: {lists}"))["id"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn register_is_stable_and_codes_are_unique() {
    let p = players();
    let (a, code_a) = join(&p, 1, "Dusty");
    let (again, code_again) = join(&p, 1, "Dusty Two");
    assert_eq!(a, again);
    assert_eq!(code_a, code_again);
    let (b, code_b) = join(&p, 2, "Mud");
    assert_ne!(a, b);
    assert_ne!(code_a, code_b);
    assert_eq!(code_a.len(), 9);
    assert_eq!(p.me(&a, NOW).unwrap()["name"], "Dusty Two");
    assert_eq!(
        p.register("short", "x", NOW, &mut Counter(0))
            .unwrap_err()
            .0,
        400
    );
    // Filtered names.
    let (c, _) = join(&p, 3, "   ");
    assert_eq!(p.me(&c, NOW).unwrap()["name"], "Rider");
    assert_eq!(p.count(), 3);
}

#[test]
fn friendships_are_mutual() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, code_b) = join(&p, 2, "Bo");
    assert_eq!(p.add(&a, Some(&code_b.to_lowercase()), None), Ok("sent"));
    assert_eq!(p.add(&a, Some(&code_b), None), Ok("sent"));
    assert_eq!(p.friends(&a, NOW).unwrap()["sent"][0]["name"], "Bo");
    assert_eq!(p.friends(&b, NOW).unwrap()["requests"][0]["name"], "Ann");
    assert!(
        p.friends(&a, NOW).unwrap()["friends"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let id = reference(&p, &b, "requests", "Ann");
    assert_eq!(p.act(&b, "accept", &id), Ok("friends"));
    for (me, other) in [(&a, "Bo"), (&b, "Ann")] {
        let lists = p.friends(me, NOW).unwrap();
        assert_eq!(lists["friends"][0]["name"], other);
        assert!(lists["requests"].as_array().unwrap().is_empty());
        assert!(lists["sent"].as_array().unwrap().is_empty());
    }
    // Remove ends it both ways.
    let id = reference(&p, &a, "friends", "Bo");
    assert_eq!(p.act(&a, "remove", &id), Ok("removed"));
    assert!(
        p.friends(&b, NOW).unwrap()["friends"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Two requests crossing make friends at once.
    let (_, code_a) = (
        0,
        p.me(&a, NOW).unwrap()["code"].as_str().unwrap().to_string(),
    );
    assert_eq!(p.add(&b, Some(&code_a), None), Ok("sent"));
    assert_eq!(p.add(&a, Some(&code_b), None), Ok("friends"));
    assert_eq!(p.add(&a, Some(&code_b), None), Ok("friends"));
    assert_eq!(p.add(&a, Some(&code_a), None), Err((400, "That's you")));
    assert_eq!(
        p.add(&a, Some("ZZZZ-ZZZZ"), None),
        Err((404, "Unknown code"))
    );
    assert_eq!(p.add(&a, Some("nope"), None), Err((400, "Bad code")));
}

#[test]
fn decline_and_cancel_drop_requests() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, code_b) = join(&p, 2, "Bo");
    p.add(&a, Some(&code_b), None).unwrap();
    let id = reference(&p, &b, "requests", "Ann");
    assert_eq!(p.act(&b, "decline", &id), Ok("declined"));
    assert!(
        p.friends(&a, NOW).unwrap()["sent"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    p.add(&a, Some(&code_b), None).unwrap();
    let id = reference(&p, &a, "sent", "Bo");
    assert_eq!(p.act(&a, "cancel", &id), Ok("cancelled"));
    assert!(
        p.friends(&b, NOW).unwrap()["requests"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(
        p.act(&a, "accept", "000000000000"),
        Err((404, "Unknown rider"))
    );
}

#[test]
fn blocks_hide_requests_and_presence() {
    let p = players();
    let (a, code_a) = join(&p, 1, "Ann");
    let (b, code_b) = join(&p, 2, "Bo");
    p.add(&a, Some(&code_b), None).unwrap();
    let id = reference(&p, &b, "requests", "Ann");
    assert_eq!(p.act(&b, "block", &id), Ok("blocked"));
    assert!(p.blocks(&a, &b) && p.blocks(&b, &a));
    assert!(
        p.friends(&a, NOW).unwrap()["sent"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // The blocked rider's request looks sent but never arrives.
    assert_eq!(p.add(&a, Some(&code_b), None), Ok("sent"));
    assert!(
        p.friends(&b, NOW).unwrap()["requests"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // The blocker cannot add without unblocking.
    assert_eq!(p.add(&b, Some(&code_a), None), Err((409, "Blocked")));
    // Blocked riders do not meet in rooms.
    p.connected(1, &a, ROOM, NOW);
    p.connected(2, &b, ROOM, NOW);
    assert!(
        p.friends(&b, NOW).unwrap()["recent"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let id = reference(&p, &b, "blocked", "Ann");
    assert_eq!(p.act(&b, "unblock", &id), Ok("unblocked"));
    assert!(!p.blocks(&a, &b));
    assert_eq!(p.add(&b, Some(&code_a), None), Ok("sent"));
}

#[test]
fn friend_lists_are_capped() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    for n in 2..=(MAX_FRIENDS as u8 + 2) {
        let (other, _) = join(&p, n, &format!("R{n}"));
        let code_a = p.me(&a, NOW).unwrap()["code"].as_str().unwrap().to_string();
        p.add(&other, Some(&code_a), None).unwrap();
    }
    // Requests beyond the cap push out the oldest.
    let lists = p.friends(&a, NOW).unwrap();
    assert_eq!(lists["requests"].as_array().unwrap().len(), MAX_REQUESTS);
    for n in 0..MAX_REQUESTS {
        let id = p.friends(&a, NOW).unwrap()["requests"][0]["id"]
            .as_str()
            .unwrap()
            .to_string();
        let state = p.act(&a, "accept", &id);
        assert_eq!(state, Ok("friends"), "{n}");
    }
    assert_eq!(
        p.friends(&a, NOW).unwrap()["friends"]
            .as_array()
            .unwrap()
            .len(),
        MAX_REQUESTS
    );
}

#[test]
fn presence_is_friends_only_and_respects_appear_offline() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, code_b) = join(&p, 2, "Bo");
    let (c, _) = join(&p, 3, "Cy");
    p.add(&a, Some(&code_b), None).unwrap();
    let id = reference(&p, &b, "requests", "Ann");
    p.act(&b, "accept", &id).unwrap();
    let freeride = atv_net::public_room_code(3).unwrap();
    p.connected(7, &b, freeride, NOW);
    let row = &p.friends(&a, NOW).unwrap()["friends"][0];
    assert_eq!(row["online"], true);
    assert_eq!(row["room"]["kind"], "freeride");
    assert_eq!(row["room"]["code"], "FREERD03");
    // Not a friend: no row at all.
    assert!(
        p.friends(&c, NOW).unwrap()["friends"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // Appear offline hides the room and the dot.
    p.settings(&b, Some(true), None).unwrap();
    let row = &p.friends(&a, NOW).unwrap()["friends"][0];
    assert_eq!(row["online"], false);
    assert!(row["room"].is_null());
    p.settings(&b, Some(false), None).unwrap();
    // Leaving the room: still online from the menus, then offline.
    p.disconnected(7, NOW);
    let row = &p.friends(&a, NOW).unwrap()["friends"][0];
    assert_eq!(row["online"], true);
    assert!(row["room"].is_null());
    let later = NOW + ONLINE_MS + 1;
    assert_eq!(p.friends(&a, later).unwrap()["friends"][0]["online"], false);
    // A private room can be joined; Quick Play shows without a code.
    p.connected(8, &b, ROOM, later);
    let row = &p.friends(&a, later).unwrap()["friends"][0];
    assert_eq!(row["room"]["kind"], "private");
    assert_eq!(row["room"]["code"], "ABCD2345");
    p.disconnected(8, later);
    p.connected(9, &b, *b"QP000001", later);
    let row = &p.friends(&a, later).unwrap()["friends"][0];
    assert_eq!(row["room"]["kind"], "quickplay");
    assert!(row["room"]["code"].is_null());
    assert_eq!(p.pid(9).as_deref(), Some(b.as_str()));
    assert_eq!(p.pid(10), None);
}

#[test]
fn recent_riders_come_from_rooms_and_add_in_one_tap() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, _) = join(&p, 2, "Bo");
    p.connected(1, &a, ROOM, NOW);
    p.connected(2, &b, ROOM, NOW + 1000);
    let lists = p.friends(&a, NOW + 5000).unwrap();
    assert_eq!(lists["recent"][0]["name"], "Bo");
    assert_eq!(lists["recent"][0]["state"], "add");
    let id = reference(&p, &a, "recent", "Bo");
    assert_eq!(p.add(&a, None, Some(&id)), Ok("sent"));
    assert_eq!(p.friends(&a, NOW).unwrap()["recent"][0]["state"], "sent");
    assert_eq!(p.friends(&b, NOW).unwrap()["recent"][0]["state"], "asked");
    // A reference only works for its own viewer.
    let (c, _) = join(&p, 3, "Cy");
    assert_eq!(p.add(&c, None, Some(&id)), Err((404, "Unknown rider")));
    // Recent riders fade after 7 days.
    let week_later = NOW + RECENT_MS + 1000;
    assert!(
        p.friends(&a, week_later).unwrap()["recent"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // At most 20, newest first.
    for n in 10..40 {
        let (other, _) = join(&p, n, &format!("R{n}"));
        p.connected(u64::from(n) + 100, &other, ROOM, NOW + u64::from(n));
    }
    let recent = p.friends(&a, NOW + 100).unwrap()["recent"].clone();
    assert_eq!(recent.as_array().unwrap().len(), RECENT);
    assert_eq!(recent[0]["name"], "R39");
}

#[test]
fn regenerated_codes_retire_the_old_one() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, code_b) = join(&p, 2, "Bo");
    let fresh = p.new_code(&b, &mut Counter(99)).unwrap();
    assert_ne!(fresh, code_b);
    assert_eq!(p.add(&a, Some(&code_b), None), Err((404, "Unknown code")));
    assert_eq!(p.add(&a, Some(&fresh), None), Ok("sent"));
    let (_, again) = join(&p, 2, "Bo");
    assert_eq!(again, fresh);
}

fn record(riders: &[(&Option<String>, bool, u32, bool, bool)]) -> MatchRecord {
    MatchRecord {
        t: NOW,
        room: "QP000001".into(),
        playlist: "race".into(),
        mode: 3,
        map: "nat01".into(),
        verified: true,
        riders: riders
            .iter()
            .enumerate()
            .map(|(i, &(pid, cpu, place, dnf, left))| MatchRider {
                pid: pid.clone(),
                name: format!("R{i}"),
                cpu,
                place,
                value: 180_000 + u64::from(place) * 1000,
                dnf,
                left,
            })
            .collect(),
    }
}

#[test]
fn verified_matches_count_stats_rating_points_and_levels() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, _) = join(&p, 2, "Bo");
    let (a, b) = (Some(a), Some(b));
    let none = None;
    let race = record(&[
        (&a, false, 1, false, false),
        (&none, true, 2, false, false),
        (&b, false, 3, false, false),
        (&none, true, 4, false, false),
    ]);
    assert_eq!(p.ingest(&race), 2);
    let ann = p.me(a.as_deref().unwrap(), NOW).unwrap();
    assert_eq!(ann["stats"]["races"], 1);
    assert_eq!(ann["stats"]["wins"], 1);
    assert_eq!(ann["stats"]["podiums"], 1);
    assert_eq!(ann["stats"]["best"][0]["value"], 181_000);
    assert_eq!(ann["league"]["points"], 10 + 15 + 10);
    assert_eq!(ann["xp"], 35);
    let bo = p.me(b.as_deref().unwrap(), NOW).unwrap();
    assert_eq!(bo["stats"]["wins"], 0);
    assert_eq!(bo["stats"]["podiums"], 1);
    assert_eq!(bo["league"]["points"], 15);
    let ra = p.rating(a.as_deref().unwrap()).unwrap();
    let rb = p.rating(b.as_deref().unwrap()).unwrap();
    assert!(ra.mu > rating::MU && rb.mu < rating::MU);
    // A slower time keeps the best; a faster one replaces it.
    let mut slower = race.clone();
    slower.riders[0].value = 200_000;
    p.ingest(&slower);
    let mut faster = race.clone();
    faster.riders[0].value = 170_000;
    p.ingest(&faster);
    let ann = p.me(a.as_deref().unwrap(), NOW).unwrap();
    assert_eq!(ann["stats"]["best"][0]["value"], 170_000);
    assert_eq!(ann["stats"]["races"], 3);
    // Unverified records count nothing.
    let mut unverified = race.clone();
    unverified.verified = false;
    assert_eq!(p.ingest(&unverified), 0);
    // Unknown ids and CPUs with ids are ignored.
    let stranger = Some("f".repeat(32));
    let cpu_with_id = record(&[
        (&stranger, false, 1, false, false),
        (&a, true, 2, false, false),
    ]);
    assert_eq!(p.ingest(&cpu_with_id), 0);
}

#[test]
fn leavers_rank_last_and_earn_nothing() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, _) = join(&p, 2, "Bo");
    let (c, _) = join(&p, 3, "Cy");
    let (a, b, c) = (Some(a), Some(b), Some(c));
    // Ann left while leading on places; Bo did not finish; Cy won.
    p.ingest(&record(&[
        (&a, false, 1, false, true),
        (&b, false, 2, true, false),
        (&c, false, 3, false, false),
    ]));
    let ra = p.rating(a.as_deref().unwrap()).unwrap();
    let rb = p.rating(b.as_deref().unwrap()).unwrap();
    let rc = p.rating(c.as_deref().unwrap()).unwrap();
    assert!(rc.mu > rb.mu && rb.mu > ra.mu, "{ra:?} {rb:?} {rc:?}");
    let ann = p.me(a.as_deref().unwrap(), NOW).unwrap();
    assert_eq!(ann["league"]["points"], 0);
    assert_eq!(ann["stats"]["wins"], 0);
    assert_eq!(ann["stats"]["races"], 1);
}

#[test]
fn league_weeks_reset_and_tiers_only_climb() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, _) = join(&p, 2, "Bo");
    let (sa, sb) = (Some(a.clone()), Some(b.clone()));
    let win = record(&[(&sa, false, 1, false, false), (&sb, false, 2, false, false)]);
    for _ in 0..4 {
        p.ingest(&win);
    }
    let ann = p.me(&a, NOW).unwrap();
    assert_eq!(ann["league"]["points"], 4 * 25);
    assert_eq!(ann["league"]["tier"], "silver");
    let board = p.league(Some(&b), NOW);
    assert_eq!(board["top"][0]["name"], "Ann");
    assert_eq!(board["top"][1]["me"], true);
    assert_eq!(board["me"]["rank"], 2);
    assert_eq!(board["me"]["top"], 100);
    assert_eq!(board["riders"], 2);
    // Monday 00:00 UTC: points and tier start over; XP and level stay.
    let next_week = league::week_start_ms(league::week(NOW) + 1);
    let ann = p.me(&a, next_week).unwrap();
    assert_eq!(ann["league"]["points"], 0);
    assert_eq!(ann["league"]["tier"], "bronze");
    assert_eq!(ann["xp"], 100);
    assert_eq!(ann["level"], 2);
    assert!(
        p.league(None, next_week)["top"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    // A late record from the past week adds XP but no points this week.
    let mut new_week = win.clone();
    new_week.t = next_week + 1;
    p.ingest(&new_week);
    p.ingest(&win);
    let ann = p.me(&a, next_week + 2).unwrap();
    assert_eq!(ann["league"]["points"], 25);
    assert_eq!(ann["xp"], 150);
}

#[test]
fn state_saves_and_loads_with_caps() {
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, code_b) = join(&p, 2, "Bo");
    p.add(&a, Some(&code_b), None).unwrap();
    let id = reference(&p, &b, "requests", "Ann");
    p.act(&b, "accept", &id).unwrap();
    let bytes = p.snapshot(NOW).expect("dirty");
    assert!(p.snapshot(NOW).is_none(), "saved once");
    let q = players();
    assert_eq!(q.load(&bytes, NOW).unwrap(), 2);
    assert_eq!(q.friends(&a, NOW).unwrap()["friends"][0]["name"], "Bo");
    assert_eq!(q.add(&a, Some(&code_b), None), Ok("friends"));
    // Idle riders without friends are dropped a long time later.
    let (c, _) = join(&q, 3, "Cy");
    let bytes = {
        q.settings(&c, Some(false), None).unwrap();
        q.snapshot(NOW).unwrap()
    };
    let r = players();
    assert_eq!(r.load(&bytes, NOW + IDLE_DROP_MS + 1).unwrap(), 2);
    assert!(r.rating(&c).is_none());
    assert!(players().load(b"not json", NOW).is_err());
}

#[test]
fn match_files_are_tailed_once_whole_lines_only() {
    let dir = std::env::temp_dir().join(format!("fury-players-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join(MATCHES_FILE);
    let p = players();
    let (a, _) = join(&p, 1, "Ann");
    let (b, _) = join(&p, 2, "Bo");
    let (sa, sb) = (Some(a.clone()), Some(b));
    let line = serde_json::to_string(&record(&[
        (&sa, false, 1, false, false),
        (&sb, false, 2, false, false),
    ]))
    .unwrap();
    std::fs::write(&path, format!("{line}\n{{\"bad\"\n{}", &line[..20])).unwrap();
    assert_eq!(p.tail(&path), 1);
    assert_eq!(p.tail(&path), 0);
    // The partial line completes.
    let mut text = std::fs::read_to_string(&path).unwrap();
    text.push_str(&line[20..]);
    text.push('\n');
    std::fs::write(&path, &text).unwrap();
    assert_eq!(p.tail(&path), 1);
    assert_eq!(p.me(&a, NOW).unwrap()["stats"]["races"], 2);
    // A rotated (shorter) file is read from the start.
    std::fs::write(&path, format!("{line}\n")).unwrap();
    assert_eq!(p.tail(&path), 1);
    assert_eq!(p.tail(&dir.join("missing.jsonl")), 0);
    let _ = std::fs::remove_dir_all(&dir);
}

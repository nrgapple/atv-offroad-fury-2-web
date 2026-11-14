// Rider lists stay vectors so steps can add and remove riders.
#![allow(clippy::useless_vec)]
use super::*;
use atv_net::decode;

fn rider(peer: u8, position: [f32; 3]) -> Rider {
    Rider {
        peer,
        conn: 100 + u64::from(peer),
        hub: true,
        position: Some(position),
        moving: true,
    }
}

/// Hub packets sent to `peer`'s connection.
fn to(out: &[Out], peer: u8) -> Vec<Packet> {
    out.iter()
        .filter_map(|o| match o {
            Out::Send(id, b) if *id == 100 + u64::from(peer) => Some(decode(b).unwrap()),
            _ => None,
        })
        .collect()
}

fn challenges(out: &[Out], peer: u8) -> Vec<Challenge> {
    to(out, peer)
        .into_iter()
        .filter_map(|p| match p {
            Packet::Challenge(c) => Some(c),
            _ => None,
        })
        .collect()
}

fn events(out: &[Out], peer: u8) -> Vec<HubPacket> {
    to(out, peer)
        .into_iter()
        .filter_map(|p| match p {
            Packet::Hub(h) if h.op == hub_op::EVENT => Some(*h),
            _ => None,
        })
        .collect()
}

fn votes(out: &[Out], peer: u8) -> Vec<HubPacket> {
    to(out, peer)
        .into_iter()
        .filter_map(|p| match p {
            Packet::Hub(h) if h.op == hub_op::VOTE => Some(*h),
            _ => None,
        })
        .collect()
}

/// A hub with no event or rotation due, and spots along +x every 50 ft.
fn quiet_hub(now: u64) -> Hub {
    let mut hub = Hub::new(42, now);
    hub.next_event = u64::MAX;
    hub.next_rotation = None;
    hub.spots = (1..40).map(|i| [i as f32 * 50., 0., 0.]).collect();
    hub
}

fn ask(hub: &mut Hub, from: u8, to: u8, riders: &[Rider], now: u64) -> Vec<Out> {
    let mut out = Vec::new();
    hub.challenge(
        from,
        Challenge {
            op: challenge_op::ASK,
            peer: to,
            ..Default::default()
        },
        riders,
        now,
        &mut out,
    );
    out
}

fn answer(hub: &mut Hub, from: u8, op: u8, serial: u32, riders: &[Rider], now: u64) -> Vec<Out> {
    let mut out = Vec::new();
    hub.challenge(
        from,
        Challenge {
            op,
            serial,
            ..Default::default()
        },
        riders,
        now,
        &mut out,
    );
    out
}

#[test]
fn an_accepted_challenge_races_to_a_waypoint_and_the_first_there_wins() {
    let mut now = 10_000;
    let mut hub = quiet_hub(now);
    let mut riders = vec![rider(0, [0., 0., 0.]), rider(1, [0., 0., 60.])];
    let out = ask(&mut hub, 0, 1, &riders, now);
    let wait = challenges(&out, 0);
    let offer = challenges(&out, 1);
    assert_eq!(wait[0].op, challenge_op::WAIT);
    assert_eq!(wait[0].peer, 1);
    assert_eq!(offer[0].op, challenge_op::OFFER);
    assert_eq!(offer[0].peer, 0);
    assert_eq!(offer[0].timer_ms, OFFER_MS as u32);
    let serial = offer[0].serial;
    // Only the challenged rider accepts.
    assert!(answer(&mut hub, 0, challenge_op::ACCEPT, serial, &riders, now).is_empty());
    now += 2_000;
    let out = answer(&mut hub, 1, challenge_op::ACCEPT, serial, &riders, now);
    let start = challenges(&out, 0)[0];
    assert_eq!(start.op, challenge_op::START);
    assert_eq!(start.peer, 1);
    assert_eq!(challenges(&out, 1)[0].target, start.target);
    let target = start.target;
    let gap = flat(target, [0., 0., 30.]);
    assert!((SPRINT_NEAR..=SPRINT_FAR).contains(&gap), "{gap}");
    assert!((30_000..=60_000).contains(&start.timer_ms));
    // Rider 1 drives there at 80 ft/s; rider 0 stays put.
    let mut result = None;
    for _ in 0..200 {
        now += 100;
        let p = riders[1].position.unwrap();
        let d = [target[0] - p[0], 0., target[2] - p[2]];
        let len = (d[0] * d[0] + d[2] * d[2]).sqrt().max(1.);
        let step = 8f32.min(len);
        riders[1].position = Some([p[0] + d[0] / len * step, 0., p[2] + d[2] / len * step]);
        let mut out = Vec::new();
        hub.step(now, 8, &riders, &mut out);
        if let Some(r) = challenges(&out, 0).first() {
            result = Some(*r);
            assert_eq!(challenges(&out, 1)[0].detail, 1);
            break;
        }
    }
    let result = result.expect("a result");
    assert_eq!(result.op, challenge_op::RESULT);
    assert_eq!(result.detail, 1);
    assert!(hub.duels.is_empty());
    // The pair cools down for a minute, then may race again.
    let out = ask(&mut hub, 1, 0, &riders_near(), now + 1_000);
    assert_eq!(challenges(&out, 1)[0].detail, challenge_end::COOLDOWN);
    let out = ask(&mut hub, 1, 0, &riders_near(), now + COOLDOWN_MS + 1);
    assert_eq!(challenges(&out, 0)[0].op, challenge_op::OFFER);
}

fn riders_near() -> Vec<Rider> {
    vec![rider(0, [0., 0., 0.]), rider(1, [0., 0., 60.])]
}

#[test]
fn a_teleport_does_not_win_a_sprint() {
    let mut now = 10_000;
    let mut hub = quiet_hub(now);
    let mut riders = riders_near();
    let serial = challenges(&ask(&mut hub, 0, 1, &riders, now), 1)[0].serial;
    let out = answer(&mut hub, 1, challenge_op::ACCEPT, serial, &riders, now);
    let target = challenges(&out, 0)[0].target;
    now += 100;
    riders[0].position = Some(target);
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    assert!(challenges(&out, 0).is_empty(), "a jump of 300+ ft in 0.1 s");
    // It counts only once 250 ft/s could have covered it.
    let need = (flat(target, [0., 0., 0.]) / MAX_SPEED * 1000.) as u64;
    now += need.saturating_sub(300);
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    assert!(challenges(&out, 0).is_empty());
    now += 400;
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    assert_eq!(challenges(&out, 0)[0].detail, 0);
}

#[test]
fn declines_expiries_and_refusals_close_the_challenge() {
    let now = 10_000;
    let mut hub = quiet_hub(now);
    let riders = riders_near();
    let serial = challenges(&ask(&mut hub, 0, 1, &riders, now), 1)[0].serial;
    let out = answer(
        &mut hub,
        1,
        challenge_op::DECLINE,
        serial,
        &riders,
        now + 500,
    );
    for peer in [0, 1] {
        let c = challenges(&out, peer)[0];
        assert_eq!(
            (c.op, c.detail),
            (challenge_op::CLOSED, challenge_end::DECLINED)
        );
    }
    // An unanswered offer expires after 8 s.
    let mut hub = quiet_hub(now);
    challenges(&ask(&mut hub, 0, 1, &riders, now), 1);
    let mut out = Vec::new();
    hub.step(now + OFFER_MS - 200, 8, &riders, &mut out);
    assert!(challenges(&out, 0).is_empty());
    hub.step(now + OFFER_MS, 8, &riders, &mut out);
    assert_eq!(challenges(&out, 0)[0].detail, challenge_end::EXPIRED);
    assert_eq!(challenges(&out, 1)[0].detail, challenge_end::EXPIRED);
    // Too far apart, oneself, a build without the hub, and a busy rider.
    let mut hub = quiet_hub(now);
    let far = vec![rider(0, [0., 0., 0.]), rider(1, [0., 0., 400.])];
    assert_eq!(
        challenges(&ask(&mut hub, 0, 1, &far, now), 0)[0].detail,
        challenge_end::UNAVAILABLE
    );
    assert_eq!(
        challenges(&ask(&mut hub, 0, 0, &riders, now), 0)[0].detail,
        challenge_end::UNAVAILABLE
    );
    let mut old = riders_near();
    old[1].hub = false;
    let out = ask(&mut hub, 0, 1, &old, now);
    assert_eq!(challenges(&out, 0)[0].detail, challenge_end::UNAVAILABLE);
    assert!(to(&out, 1).is_empty(), "nothing for an older build");
    let three = vec![riders[0], riders[1], rider(2, [30., 0., 0.])];
    challenges(&ask(&mut hub, 0, 1, &three, now), 1);
    assert_eq!(
        challenges(&ask(&mut hub, 2, 1, &three, now), 2)[0].detail,
        challenge_end::BUSY
    );
    // A rider who leaves ends the challenge for the other.
    let mut out = Vec::new();
    hub.step(now + 200, 8, &three[..1], &mut out);
    assert_eq!(challenges(&out, 0)[0].detail, challenge_end::LEFT);
}

#[test]
fn a_pop_up_event_opens_a_join_circle_and_runs_a_hill_for_its_joiners() {
    let mut now = 10_000;
    let mut hub = quiet_hub(now);
    hub.next_event = now;
    hub.next_kind = event_kind::HILL;
    let mut riders = vec![
        rider(0, [0., 0., 0.]),
        rider(1, [500., 0., 0.]),
        rider(2, [900., 0., 900.]),
    ];
    riders[2].hub = false;
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    let open = events(&out, 0);
    let open = open.last().expect("announced");
    assert_eq!(open.phase, event_phase::JOIN);
    assert_eq!(open.kind, event_kind::HILL);
    assert_eq!(open.timer_ms, 45_000);
    assert_eq!(open.radius, JOIN_RADIUS);
    assert!(to(&out, 2).is_empty(), "older builds are not interrupted");
    let point = open.point;
    // Riders 0 and 1 drive into the circle.
    riders[0].position = Some(point);
    riders[1].position = Some([point[0] + 10., 0., point[2]]);
    now += 200;
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    assert_eq!(events(&out, 0).last().unwrap().mask, 0b11);
    // The window closes: the hill is the highest spot near the circle.
    hub.spots.push([point[0] + 100., 40., point[2]]);
    now += 45_000;
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    let run = events(&out, 0).last().unwrap().clone();
    assert_eq!(run.phase, event_phase::RUN);
    assert_eq!(run.choice, 0, "two joined: not solo");
    assert_eq!(run.target, [point[0] + 100., 40., point[2]]);
    assert_eq!(run.radius, HILL_RADIUS);
    // Rider 1 holds the hill for 20 s (driving there at a believable speed).
    for k in 0..400 {
        now += 100;
        if k == 0 {
            riders[1].position = Some([point[0] + 30., 20., point[2]]);
        } else if k == 5 {
            riders[1].position = Some(run.target);
        }
        let mut out = Vec::new();
        hub.step(now, 8, &riders, &mut out);
        if k == 205 {
            riders[1].position = Some([point[0] + 50., 0., point[2]]);
        }
    }
    now += 30_000;
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    let results = events(&out, 1).last().unwrap().clone();
    assert_eq!(results.phase, event_phase::RESULTS);
    assert_eq!(results.rows[0].peer, 1);
    assert!(
        (19.5..=20.6).contains(&results.rows[0].value),
        "{:?}",
        results.rows
    );
    assert_eq!(results.rows[0].points, 100);
    assert_eq!((results.rows[1].peer, results.rows[1].points), (0, 60));
    assert!(hub.event.is_none());
    assert!(hub.next_event >= now + 4 * 60_000);
}

#[test]
fn a_lone_joiner_rides_a_solo_sprint_and_an_empty_circle_is_cancelled() {
    let mut now = 10_000;
    let mut hub = quiet_hub(now);
    hub.next_event = now;
    hub.next_kind = event_kind::SPRINT;
    let mut riders = vec![rider(0, [0., 0., 0.]), rider(1, [1500., 0., 0.])];
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    let point = events(&out, 0)[0].point;
    riders[0].position = Some(point);
    now += 45_100;
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    let run = events(&out, 1).last().unwrap().clone();
    assert_eq!(
        (run.phase, run.choice),
        (event_phase::RUN, atv_net::hub::SOLO)
    );
    assert_eq!(run.mask, 1);
    let gap = flat(run.target, point);
    assert!((400. ..=1000.).contains(&gap), "{gap}");
    // Rider 0 reaches it in 10 s; the solo sprint ends at once.
    for k in 1..=100 {
        now += 100;
        let t = k as f32 / 100.;
        riders[0].position = Some([
            point[0] + (run.target[0] - point[0]) * t,
            0.,
            point[2] + (run.target[2] - point[2]) * t,
        ]);
        let mut out = Vec::new();
        hub.step(now, 8, &riders, &mut out);
        if let Some(done) = events(&out, 0)
            .into_iter()
            .find(|e| e.phase == event_phase::RESULTS)
        {
            assert!(done.rows[0].finished);
            assert!((9.0..=10.2).contains(&done.rows[0].value));
            assert_eq!(done.rows[0].points, 50);
            break;
        }
        assert!(k < 100, "the sprint ends");
    }
    // Nobody joins the next one.
    hub.next_event = now;
    let mut out = Vec::new();
    hub.step(now + 100, 8, &riders, &mut out);
    assert_eq!(events(&out, 0)[0].phase, event_phase::JOIN);
    let mut out = Vec::new();
    hub.step(now + 46_000, 8, &riders, &mut out);
    assert_eq!(events(&out, 0)[0].phase, event_phase::CANCELLED);
    assert!(hub.event.is_none());
}

#[test]
fn stunt_off_scores_only_add_and_rank_the_results() {
    let mut now = 10_000;
    let mut hub = quiet_hub(now);
    hub.next_event = now;
    hub.next_kind = event_kind::STUNTS;
    let riders = vec![rider(0, [0., 0., 0.]), rider(1, [0., 0., 10.])];
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    let open = events(&out, 0)[0].clone();
    assert!(flat(open.point, [0., 0., 0.]) <= 260.);
    let inside = vec![rider(0, open.point), rider(1, open.point)];
    now += 45_100;
    hub.step(now, 8, &inside, &mut Vec::new());
    let score = |hub: &mut Hub, peer, value: f32| {
        hub.message(
            peer,
            HubPacket {
                op: hub_op::SCORE,
                serial: open.serial,
                value,
                ..Default::default()
            },
            &inside,
            now,
            &mut Vec::new(),
        );
    };
    score(&mut hub, 0, 5_000.);
    score(&mut hub, 1, 9_000.);
    score(&mut hub, 1, 2_000.);
    score(&mut hub, 0, 1.0e9);
    now += EVENT_MS;
    let mut out = Vec::new();
    hub.step(now, 8, &inside, &mut out);
    let results = events(&out, 0).last().unwrap().clone();
    assert_eq!(results.phase, event_phase::RESULTS);
    let rows: Vec<(u8, f32, u16)> = results
        .rows
        .iter()
        .map(|r| (r.peer, r.value, r.points))
        .collect();
    assert_eq!(rows, vec![(1, 9_000., 100), (0, 5_000., 60)]);
}

#[test]
fn the_rotation_vote_moves_the_room_and_ends_what_runs() {
    let mut now = 10_000;
    let mut hub = quiet_hub(now);
    hub.next_rotation = Some(now + 120_000);
    let riders = riders_near();
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    assert!(votes(&out, 0).is_empty());
    // A minute before the move, three other venues go to the vote.
    now += 60_000;
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    let vote = votes(&out, 1)[0].clone();
    assert_eq!(vote.maps.len(), 3);
    assert!(
        vote.maps
            .iter()
            .all(|m| ROTATION_MAPS.contains(m) && *m != 8)
    );
    assert_eq!(vote.timer_ms, 60_000);
    // No challenges while the room is about to move.
    let ask_out = ask(&mut hub, 0, 1, &riders, now);
    assert_eq!(challenges(&ask_out, 0)[0].detail, challenge_end::BUSY);
    for (peer, choice) in [(0, 2), (1, 2)] {
        let mut out = Vec::new();
        hub.message(
            peer,
            HubPacket {
                op: hub_op::CAST,
                serial: vote.serial,
                choice,
                ..Default::default()
            },
            &riders,
            now,
            &mut out,
        );
        assert_eq!(votes(&out, 0)[0].votes[2], peer + 1);
    }
    now += 60_000;
    let mut out = Vec::new();
    let moved = hub.step(now, 8, &riders, &mut out);
    assert_eq!(moved.map, Some(vote.maps[2]));
    let last = votes(&out, 0)[0].clone();
    assert_eq!(last.choice, 2);
    assert!(hub.vote.is_none());
    assert!(hub.spots.is_empty(), "spots belong to the old venue");
    assert!(hub.next_rotation.unwrap() >= now + 20 * 60_000);
}

#[test]
fn newcomers_learn_the_hub_riders_and_the_event_in_progress() {
    let now = 10_000;
    let mut hub = quiet_hub(now);
    hub.next_event = now;
    let riders = vec![rider(0, [0., 0., 0.])];
    let mut out = Vec::new();
    hub.step(now, 8, &riders, &mut out);
    let first = to(&out, 0);
    assert!(matches!(&first[0], Packet::Hub(h) if h.op == hub_op::RIDERS && h.mask == 1));
    let mut more = riders.clone();
    more.push(rider(3, [10., 0., 0.]));
    let mut out = Vec::new();
    hub.step(now + 200, 8, &more, &mut out);
    let joined = to(&out, 3);
    assert!(matches!(&joined[0], Packet::Hub(h) if h.op == hub_op::RIDERS && h.mask == 0b1001));
    assert!(
        matches!(&joined[1], Packet::Hub(h) if h.op == hub_op::EVENT && h.phase == event_phase::JOIN)
    );
    assert_eq!(to(&out, 0).len(), 1, "rider 0 hears only the new mask");
}

#[test]
fn hub_settings_parse_ranges_and_off() {
    assert_eq!(
        crate::config::Config::from_vars(|n| match n {
            "FURY_HUB_EVENT_SECS" => Some("20-30".into()),
            "FURY_HUB_JOIN_SECS" => Some("10".into()),
            "FURY_HUB_ROTATION_SECS" => Some("off".into()),
            _ => None,
        })
        .unwrap()
        .hub,
        HubConfig {
            events: Some((20_000, 30_000)),
            join_ms: 10_000,
            rotation: None,
        }
    );
    assert_eq!(
        crate::config::Config::from_vars(|_| None).unwrap().hub,
        HubConfig::default()
    );
    for (name, value) in [
        ("FURY_HUB_EVENT_SECS", "30-20"),
        ("FURY_HUB_JOIN_SECS", "0"),
        ("FURY_HUB_ROTATION_SECS", "soon"),
    ] {
        assert!(
            crate::config::Config::from_vars(|n| (n == name).then(|| value.into())).is_err(),
            "{name}={value}"
        );
    }
}

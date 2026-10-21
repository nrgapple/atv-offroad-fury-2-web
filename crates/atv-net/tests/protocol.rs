// Presentation only: not part of the deterministic simulation (clippy.toml).
#![allow(clippy::disallowed_methods)]
use atv_net::*;

fn participant(id: u8) -> Participant {
    Participant {
        id,
        name: format!("Rider {id}"),
        atv_model: 1,
        rider_model: 1,
        appearance: [0; 8],
    }
}
fn lobby() -> Lobby {
    Lobby {
        host: 0,
        epoch: 1,
        map: 4,
        started: false,
        public: false,
        players: (0..4).map(participant).collect(),
    }
}
fn snapshot(sequence: u32, time_ms: u32, x: f32) -> Snapshot {
    let mut s = Snapshot {
        sequence,
        time_ms,
        rpm: 6000.,
        speed: 20.,
        throttle: 0.8,
        ..Snapshot::default()
    };
    s.atv.position[0] = x;
    s.velocity = [20., 0., 0.];
    s.rider.position[0] = x + 1.;
    s.rider_velocity = [20., 0., 0.];
    s.atv_nodes = vec![Transform::default(); 27];
    s.rider_nodes = vec![Transform::default(); 18];
    s
}
fn rewrite_length(bytes: &mut [u8]) {
    let length = (bytes.len() - 8) as u16;
    bytes[6..8].copy_from_slice(&length.to_le_bytes());
}
#[test]
fn every_control_and_full_pose_roundtrips() {
    let code = *b"ABCD2345";
    let packets = [
        Packet::Join {
            code,
            player: participant(0),
        },
        Packet::Welcome {
            peer: 1,
            code,
            lobby: lobby(),
            key: [7; 16],
        },
        Packet::Lobby(lobby()),
        Packet::SetLobby {
            epoch: 1,
            map: 4,
            started: true,
        },
        Packet::Snapshot((snapshot(1, 100, 2.)).into()),
        Packet::Leave { peer: 3 },
        Packet::Ping { nonce: u32::MAX },
        Packet::Pong {
            nonce: u32::MAX,
            server_ms: 1_791_000_000_123,
        },
        Packet::Profile(participant(2)),
        Packet::Ready { map: 40 },
    ];
    for packet in packets {
        assert_eq!(decode(&encode(&packet).unwrap()).unwrap(), packet);
    }
}
#[test]
fn pose_packet_budget_covers_all_character_models() {
    let mut s = snapshot(1, 0, 0.);
    assert_eq!(
        encode(&Packet::Snapshot((s.clone()).into())).unwrap().len(),
        2526
    );
    s.rider_nodes = vec![Transform::default(); 27];
    assert_eq!(
        encode(&Packet::Snapshot((s.clone()).into())).unwrap().len(),
        2994
    );
    s.rider_nodes = vec![Transform::default(); 64];
    s.atv_nodes = vec![Transform::default(); 64];
    assert_eq!(encode(&Packet::Snapshot((s).into())).unwrap().len(), 6842);
}
#[test]
fn truncated_snapshot_never_decodes_or_panics() {
    let bytes = encode(&Packet::Snapshot((snapshot(1, 0, 0.)).into())).unwrap();
    for len in 0..bytes.len() {
        let mut cut = bytes[..len].to_vec();
        if len >= 8 {
            rewrite_length(&mut cut);
        }
        assert!(decode(&cut).is_err(), "accepted truncation {len}");
    }
}
#[test]
fn rejects_trailing_wrong_version_kind_magic_and_oversize() {
    let bytes = encode(&Packet::Ping { nonce: 1 }).unwrap();
    for (offset, value) in [(0, 0), (4, 1), (5, 255)] {
        let mut bad = bytes.clone();
        bad[offset] = value;
        assert!(decode(&bad).is_err());
    }
    let mut bad = bytes.clone();
    bad.push(0);
    rewrite_length(&mut bad);
    assert_eq!(decode(&bad), Err(Error::InvalidLength));
    assert_eq!(
        decode(&vec![0; MAX_PACKET_BYTES + 1]),
        Err(Error::InvalidLength)
    );
}
#[test]
fn rejects_nonfinite_out_of_bounds_and_zero_quaternion() {
    let packet = Packet::Snapshot((snapshot(1, 0, 0.)).into());
    for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 1_000_001.] {
        let mut bytes = encode(&packet).unwrap();
        bytes[21..25].copy_from_slice(&invalid.to_le_bytes());
        assert!(decode(&bytes).is_err());
    }
    let mut s = snapshot(1, 0, 0.);
    s.atv.rotation = [0.; 4];
    assert!(s.validate().is_err());
    s.atv = Transform::default();
    s.throttle = 1.1;
    assert!(s.validate().is_err());
    s.throttle = 0.;
    s.rpm = -1.;
    assert!(s.validate().is_err());
    s.rpm = 0.;
    s.angular_velocity[0] = 1001.;
    assert!(s.validate().is_err());
}
#[test]
fn rejects_unbounded_node_count_before_allocation() {
    let mut bytes = encode(&Packet::Snapshot((snapshot(1, 0, 0.)).into())).unwrap();
    bytes[97] = 65;
    assert_eq!(decode(&bytes), Err(Error::InvalidLength));
    let mut s = snapshot(1, 0, 0.);
    s.atv_nodes.push(Transform::default());
    s.atv_nodes.resize(65, Transform::default());
    assert!(encode(&Packet::Snapshot((s).into())).is_err());
}
#[test]
fn rejects_unknown_state_boolean_invalid_roster_and_room() {
    let mut bytes = encode(&Packet::Snapshot((Snapshot::default()).into())).unwrap();
    bytes[99] = 4;
    assert_eq!(decode(&bytes), Err(Error::InvalidValue));
    let mut bytes = encode(&Packet::SetLobby {
        epoch: 1,
        map: 1,
        started: true,
    })
    .unwrap();
    bytes[14] = 2;
    assert_eq!(decode(&bytes), Err(Error::InvalidValue));
    let mut l = lobby();
    l.players[1].id = 0;
    assert!(encode(&Packet::Lobby(l)).is_err());
    let mut l = lobby();
    l.players.retain(|p| p.id != 0);
    assert!(encode(&Packet::Lobby(l)).is_err());
    assert!(
        encode(&Packet::Join {
            code: *b"abcd2345",
            player: participant(0)
        })
        .is_err()
    );
    assert!(
        encode(&Packet::Welcome {
            peer: 3,
            code: *b"ABCD2345",
            lobby: Lobby {
                players: vec![participant(0)],
                ..lobby()
            },
            key: [7; 16],
        })
        .is_err()
    );
}
#[test]
fn names_are_utf8_bounded_and_control_free() {
    let mut p = participant(0);
    p.name = "😀".repeat(8);
    let packet = Packet::Join {
        code: *b"ABCD2345",
        player: p.clone(),
    };
    assert_eq!(decode(&encode(&packet).unwrap()).unwrap(), packet);
    p.name.push('x');
    assert!(
        encode(&Packet::Join {
            code: *b"ABCD2345",
            player: p
        })
        .is_err()
    );
    let mut p = participant(0);
    p.name = "bad\nname".into();
    assert!(
        encode(&Packet::Join {
            code: *b"ABCD2345",
            player: p
        })
        .is_err()
    );
    let mut bytes = encode(&Packet::Join {
        code: *b"ABCD2345",
        player: participant(0),
    })
    .unwrap();
    bytes[18] = 255;
    assert_eq!(decode(&bytes), Err(Error::InvalidUtf8));
}
#[test]
fn arbitrary_bytes_are_rejected_without_panic() {
    let mut seed = 42u32;
    for size in [0, 8, 32, 156, 2520, 6836, 8192] {
        for _ in 0..40 {
            let mut bytes = vec![0; size];
            for b in &mut bytes {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                *b = seed as u8;
            }
            if size >= 8 {
                bytes[..4].copy_from_slice(b"ATVM");
                bytes[4] = 1;
                bytes[5] = 4;
                rewrite_length(&mut bytes);
            }
            assert!(decode(&bytes).is_err());
        }
    }
}
#[test]
fn serial_order_wraps_without_accepting_ancient_packets() {
    assert!(sequence_newer(0, u32::MAX));
    assert!(!sequence_newer(u32::MAX, 0));
    assert!(!sequence_newer(2, 2));
    assert!(!sequence_newer(0x8000_0000, 0));
}
#[test]
fn smoothing_maps_clock_origins_and_interpolates_nodes_audio() {
    let mut buffer = PoseBuffer::default();
    let a = snapshot(1, 10000, 0.);
    let mut b = snapshot(2, 10050, 1.);
    b.rpm = 7000.;
    b.atv_nodes[0].position[1] = 2.;
    buffer.push(a, 500).unwrap();
    buffer.push(b, 550).unwrap();
    let sample = buffer.sample(605).unwrap();
    assert!((sample.snapshot.atv.position[0] - 0.5).abs() < 0.0001);
    assert!((sample.snapshot.atv_nodes[0].position[1] - 1.).abs() < 0.0001);
    assert_eq!(sample.snapshot.rpm, 6500.);
    assert!(!sample.stale);
}
#[test]
fn quaternion_shortest_arc_stays_unit_and_handles_antipodal() {
    let mut buffer = PoseBuffer::default();
    let mut a = snapshot(1, 0, 0.);
    a.atv.rotation = [
        (179f32.to_radians() / 2.).cos(),
        0.,
        (179f32.to_radians() / 2.).sin(),
        0.,
    ];
    let mut b = snapshot(2, 50, 1.);
    b.atv.rotation = [
        (-179f32.to_radians() / 2.).cos(),
        0.,
        (-179f32.to_radians() / 2.).sin(),
        0.,
    ];
    buffer.push(a, 0).unwrap();
    buffer.push(b, 50).unwrap();
    let q = buffer.sample(105).unwrap().snapshot.atv.rotation;
    assert!(q[0].abs() < 0.001);
    assert!((q[2].abs() - 1.).abs() < 0.001);
    let mut b = snapshot(3, 100, 2.);
    b.atv.rotation = q.map(|v| -v);
    buffer.push(b, 100).unwrap();
    assert!(
        buffer
            .sample(155)
            .unwrap()
            .snapshot
            .atv
            .rotation
            .iter()
            .all(|v| v.is_finite())
    );
}
#[test]
fn extrapolation_rotates_and_separated_rider_moves_then_freezes() {
    let mut buffer = PoseBuffer::default();
    let mut s = snapshot(1, 100, 0.);
    s.angular_velocity = [0., std::f32::consts::PI, 0.];
    s.rider_state = RiderState::Wreck;
    s.rider_velocity = [0., 10., 0.];
    buffer.push(s, 100).unwrap();
    let near = buffer.sample(230).unwrap();
    assert_eq!(near.extrapolated_ms, 50);
    assert!((near.snapshot.atv.position[0] - 1.).abs() < 0.0001);
    assert!((near.snapshot.rider.position[1] - 0.5).abs() < 0.0001);
    assert!((near.snapshot.atv.rotation[2] - (std::f32::consts::PI * 0.025).sin()).abs() < 0.0001);
    let far = buffer.sample(2000).unwrap();
    assert_eq!(far.extrapolated_ms, 100);
    assert!(far.stale);
    assert_eq!(far.snapshot.atv.position[0], 2.);
    assert_eq!(buffer.sample(3000).unwrap().snapshot.atv, far.snapshot.atv);
}
#[test]
fn buffer_bounded_old_packets_and_clock_regression_do_not_mutate() {
    let mut buffer = PoseBuffer::default();
    for n in 0..100 {
        buffer
            .push(snapshot(n, n * 50, n as f32), u64::from(n * 50))
            .unwrap();
    }
    assert_eq!(buffer.len(), 32);
    assert!(!buffer.push(snapshot(99, 4950, 99.), 4950).unwrap());
    assert!(!buffer.push(snapshot(98, 4900, 98.), 4950).unwrap());
    assert!(buffer.push(snapshot(100, 5000, 100.), 4949).is_err());
    assert_eq!(buffer.sample(5000).unwrap().snapshot.atv.position[0], 98.4);
    buffer.clear();
    assert!(buffer.is_empty());
    assert!(buffer.sample(5000).is_none());
}
#[test]
fn timestamp_and_sequence_rollover_remain_smooth() {
    let mut buffer = PoseBuffer::default();
    buffer
        .push(snapshot(u32::MAX, u32::MAX - 24, 0.), 100)
        .unwrap();
    buffer.push(snapshot(0, 25, 1.), 150).unwrap();
    assert!((buffer.sample(205).unwrap().snapshot.atv.position[0] - 0.5).abs() < 0.0001);
    assert!(!buffer.push(snapshot(1, 24, 2.), 160).unwrap());
}
#[test]
fn crash_teleport_and_model_switch_are_discontinuous() {
    for mode in 0..3 {
        let mut buffer = PoseBuffer::default();
        let mut b = snapshot(2, 50, 1.);
        match mode {
            0 => {
                b.crash_serial = 1;
                b.rider_state = RiderState::Wreck;
            }
            1 => b.atv.position[0] = 500.,
            _ => b.atv_model = 2,
        }
        buffer.push(snapshot(1, 0, 0.), 0).unwrap();
        buffer.push(b.clone(), 50).unwrap();
        assert_eq!(buffer.sample(105).unwrap().snapshot.atv.position[0], 0.);
        assert_eq!(buffer.sample(130).unwrap().snapshot, b);
    }
}
#[test]
fn matrix_conversion_preserves_source_wxyz_basis_scale_and_reflections() {
    for axis in 1..4 {
        for degrees in [0f32, 1., 90., 179., 180., 270.] {
            for scale in [[1., 1., 1.], [2., 3., 4.], [-2., 3., 4.]] {
                let mut q = [0.; 4];
                q[0] = (degrees.to_radians() / 2.).cos();
                q[axis] = (degrees.to_radians() / 2.).sin();
                let t = Transform {
                    position: [12., -3., 50.],
                    rotation: q,
                    scale,
                    shear: [0.; 3],
                };
                let matrix = t.to_column_major();
                let rebuilt = Transform::from_column_major(matrix)
                    .unwrap()
                    .to_column_major();
                for (a, b) in matrix.iter().zip(rebuilt) {
                    assert!((a - b).abs() < 0.00001, "{axis}/{degrees}: {a} vs {b}");
                }
            }
        }
    }
    let quarter = Transform {
        rotation: [0.5f32.sqrt(), 0., 0.5f32.sqrt(), 0.],
        ..Transform::default()
    }
    .to_column_major();
    assert!((quarter[2] + 1.).abs() < 0.00001);
    assert!((quarter[8] - 1.).abs() < 0.00001);
}
#[test]
fn non_affine_and_degenerate_matrices_rejected() {
    let mut m = Transform::default().to_column_major();
    m[4] = 0.5;
    assert_eq!(
        Transform::from_column_major(m).unwrap().to_column_major(),
        m
    );
    let mut m = Transform::default().to_column_major();
    m[3] = 0.1;
    assert!(Transform::from_column_major(m).is_err());
    let mut m = Transform::default().to_column_major();
    m[0] = 0.;
    assert!(Transform::from_column_major(m).is_err());
    let mut m = Transform::default().to_column_major();
    m[0] = f32::NAN;
    assert!(Transform::from_column_major(m).is_err());
}

#[test]
fn user_codes_normalize_but_invalid_characters_do_not_pass() {
    assert_eq!(parse_room_code(" abcd2345\n").unwrap(), *b"ABCD2345");
    assert_eq!(parse_room_code(" fury2016 ").unwrap(), *b"FURY2016");
    for code in ["ABCDEFG", "ABCDEFGHI", "ABC0!234", "ABCDé234", "ABCD 234"] {
        assert!(parse_room_code(code).is_err());
    }
}
#[test]
fn sender_clock_jump_and_cross_peer_input_do_not_mutate_buffer() {
    let mut buffer = PoseBuffer::default();
    buffer.push(snapshot(1, 100, 0.), 100).unwrap();
    assert_eq!(
        buffer.push(snapshot(2, 100000, 1.), 150),
        Err(Error::InvalidValue)
    );
    let mut wrong = snapshot(2, 150, 1.);
    wrong.peer = 1;
    assert_eq!(buffer.push(wrong, 150), Err(Error::InvalidValue));
    assert_eq!(buffer.len(), 1);
    assert!(buffer.push(snapshot(2, 150, 1.), 150).unwrap());
}
#[test]
fn receipt_jitter_does_not_shift_sender_timeline() {
    let mut buffer = PoseBuffer::default();
    buffer.push(snapshot(1, 10000, 0.), 500).unwrap();
    buffer.push(snapshot(2, 10050, 1.), 575).unwrap();
    buffer.push(snapshot(3, 10100, 2.), 620).unwrap();
    assert!((buffer.sample(655).unwrap().snapshot.atv.position[0] - 1.5).abs() < 0.0001);
}

#[test]
fn qr_decomposition_preserves_affine_linkage_shear_and_interpolates_it() {
    let mut buffer = PoseBuffer::default();
    let mut left = snapshot(1, 0, 0.);
    let mut right = snapshot(2, 50, 1.);
    left.atv_nodes[0].shear = [0.1, 0.2, 0.3];
    right.atv_nodes[0].shear = [0.5, 0.6, 0.7];
    buffer.push(left, 0).unwrap();
    buffer.push(right, 50).unwrap();
    let mid = buffer.sample(105).unwrap().snapshot.atv_nodes[0];
    for (a, b) in mid.shear.iter().zip([0.3, 0.4, 0.5]) {
        assert!((a - b).abs() < 0.00001);
    }
    for degrees in [0f32, 45., 90., 180., 270.] {
        for scale in [[1., 2., 3.], [-1., 2., 3.], [1., -2., 3.], [1., 2., -3.]] {
            let t = Transform {
                position: [2., 3., 4.],
                rotation: [
                    (degrees.to_radians() / 2.).cos(),
                    0.,
                    (degrees.to_radians() / 2.).sin(),
                    0.,
                ],
                scale,
                shear: [0.2, -0.4, 0.6],
            };
            let m = t.to_column_major();
            let rebuilt = Transform::from_column_major(m).unwrap().to_column_major();
            for (a, b) in m.iter().zip(rebuilt) {
                assert!((a - b).abs() < 0.00001);
            }
        }
    }
    let mut bytes = encode(&Packet::Snapshot((Snapshot::default()).into())).unwrap();
    bytes[61..65].copy_from_slice(&f32::NAN.to_le_bytes());
    assert_eq!(decode(&bytes), Err(Error::InvalidTransform));
}

#[test]
fn all_map_control_and_roster_roundtrip_without_changing_wire_layout() {
    for map in 1..=MAX_MAP_ID {
        let control = Packet::SetLobby {
            epoch: 7,
            map,
            started: true,
        };
        let bytes = encode(&control).unwrap();
        assert_eq!(bytes.len(), 15);
        assert_eq!(bytes[5], 8);
        assert_eq!(&bytes[12..14], &map.to_le_bytes());
        assert_eq!(decode(&bytes).unwrap(), control);
        let roster = Lobby { map, ..lobby() };
        for packet in [
            Packet::Lobby(roster.clone()),
            Packet::Welcome {
                peer: 1,
                code: *b"ABCD2345",
                lobby: roster,
                key: [7; 16],
            },
        ] {
            assert_eq!(decode(&encode(&packet).unwrap()).unwrap(), packet);
        }
    }
}
#[test]
fn profile_and_ready_controls_validate_models_names_and_maps() {
    let mut bad = participant(1);
    bad.name.clear();
    assert!(encode(&Packet::Profile(bad)).is_err());
    assert_eq!(
        encode(&Packet::Ready {
            map: MAX_MAP_ID + 1
        }),
        Err(Error::InvalidValue)
    );
    let mut ready = encode(&Packet::Ready { map: 1 }).unwrap();
    ready[8..10].copy_from_slice(&0u16.to_le_bytes());
    assert_eq!(decode(&ready), Err(Error::InvalidValue));
    let mut short = encode(&Packet::Pong {
        nonce: 1,
        server_ms: 2,
    })
    .unwrap();
    short.pop();
    rewrite_length(&mut short);
    assert!(decode(&short).is_err());
}
#[test]
fn unsupported_maps_are_rejected_by_encoder_and_untrusted_wire_decoder() {
    for map in [0, MAX_MAP_ID + 1, 255, u16::MAX] {
        assert!(!valid_map(map));
        assert_eq!(
            encode(&Packet::SetLobby {
                epoch: 1,
                map,
                started: false
            }),
            Err(Error::InvalidValue)
        );
        assert_eq!(
            encode(&Packet::Lobby(Lobby { map, ..lobby() })),
            Err(Error::InvalidValue)
        );
        let mut control = encode(&Packet::SetLobby {
            epoch: 1,
            map: 9,
            started: false,
        })
        .unwrap();
        control[12..14].copy_from_slice(&map.to_le_bytes());
        assert_eq!(decode(&control), Err(Error::InvalidValue));
        let mut roster = encode(&Packet::Lobby(lobby())).unwrap();
        roster[13..15].copy_from_slice(&map.to_le_bytes());
        assert_eq!(decode(&roster), Err(Error::InvalidValue));
        let mut welcome = encode(&Packet::Welcome {
            peer: 1,
            code: *b"ABCD2345",
            lobby: lobby(),
            key: [7; 16],
        })
        .unwrap();
        welcome[22..24].copy_from_slice(&map.to_le_bytes());
        assert_eq!(decode(&welcome), Err(Error::InvalidValue));
    }
}

#[test]
fn environment_catalog_and_map_cycle_are_bounded_and_keep_source_keys() {
    for id in 1..=8 {
        assert_eq!(map_environment(id), Some(format!("env{id:02}").as_str()));
        assert_eq!(next_map(id), Some(id + 1));
    }
    assert_eq!(map_environment(9), Some("free05"));
    assert_eq!(next_map(9), Some(10));
    assert_eq!(map_environment(40), Some("game01"));
    assert_eq!(map_environment(39), Some("training"));
    assert_eq!(next_map(40), Some(1));
    for invalid in [0, MAX_MAP_ID + 1, u16::MAX] {
        assert_eq!(map_environment(invalid), None);
        assert_eq!(next_map(invalid), None);
    }
}

#[test]
fn train_platform_roundtrip_and_untrusted_bounds() {
    let mut s = snapshot(1, 100, 12.);
    s.train_time = Some(120.5);
    s.train_running = true;
    s.platform = Some(Box::new(Platform {
        car: 10,
        transform: Transform::default(),
    }));
    let packet = Packet::Snapshot((s.clone()).into());
    assert_eq!(decode(&encode(&packet).unwrap()).unwrap(), packet);
    s.platform.as_mut().unwrap().car = 32;
    assert!(s.validate().is_err());
    s.platform.as_mut().unwrap().car = 1;
    s.train_time = Some(f32::NAN);
    assert!(s.validate().is_err());
    s.train_time = None;
    assert!(s.validate().is_err());
}
#[test]
fn attached_rider_does_not_extrapolate_away_from_car_and_dismount_is_discrete() {
    let mut s = snapshot(1, 100, 12.);
    s.train_time = Some(10.);
    s.platform = Some(Box::new(Platform {
        car: 1,
        transform: Transform::default(),
    }));
    let mut b = PoseBuffer::default();
    b.push(s.clone(), 100).unwrap();
    assert_eq!(b.sample(300).unwrap().snapshot.atv.position, s.atv.position);
    s.sequence = 2;
    s.time_ms = 150;
    s.platform = None;
    s.atv.position[0] = 15.;
    b.push(s, 150).unwrap();
    assert!(b.sample(200).unwrap().snapshot.platform.is_some());
    assert!(b.sample(230).unwrap().snapshot.platform.is_none());
}

#[test]
fn rider_appearance_roundtrips_and_rejects_out_of_range_gear() {
    let mut l = lobby();
    l.players[1].appearance = [3, 1, 15, 26, 26, 26, 11, 6];
    let packet = Packet::Lobby(l.clone());
    assert_eq!(decode(&encode(&packet).unwrap()).unwrap(), packet);
    l.players[1].appearance[7] = 16;
    assert!(encode(&Packet::Lobby(l.clone())).is_err());
    l.players[1].appearance = [65, 0, 0, 0, 0, 0, 0, 0];
    assert!(encode(&Packet::Lobby(l)).is_err());
}

#[test]
fn shared_free_ride_lobbies_hold_twenty_and_private_rooms_keep_four() {
    let public = Lobby {
        host: 19,
        map: PUBLIC_MAP,
        started: true,
        public: true,
        players: (0..PUBLIC_CAPACITY as u8).map(participant).collect(),
        ..lobby()
    };
    let bytes = encode(&Packet::Lobby(public.clone())).unwrap();
    assert!(bytes.len() < MAX_PACKET_BYTES);
    assert_eq!(decode(&bytes).unwrap(), Packet::Lobby(public.clone()));
    // Slot 20 does not exist, even in a shared room.
    let mut over = public.clone();
    over.players[19].id = 20;
    assert!(encode(&Packet::Lobby(over)).is_err());
    // A private room never exceeds the original four riders.
    let private = Lobby {
        public: false,
        players: (0..5).map(participant).collect(),
        ..lobby()
    };
    assert!(encode(&Packet::Lobby(private)).is_err());
    // Unknown lobby flag bits are rejected.
    let mut flags = encode(&Packet::Lobby(lobby())).unwrap();
    let offset = 8 + 1 + 4 + 2;
    flags[offset] = 4;
    assert!(decode(&flags).is_err());
}

#[test]
fn shared_room_codes_are_numbered_and_reserved() {
    assert_eq!(public_room_code(1), Some(*b"FREERD01"));
    assert_eq!(public_room_code(PUBLIC_ROOMS), Some(*b"FREERD25"));
    assert_eq!(public_room_code(0), None);
    assert_eq!(public_room_code(PUBLIC_ROOMS + 1), None);
    let code = public_room_code(7).unwrap();
    assert!(valid_room_code(&code) && is_public_room_code(&code));
    assert!(!is_public_room_code(b"ABCD2345"));
}

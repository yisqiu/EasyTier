use super::*;
use crate::common::global_ctx::tests::get_mock_global_ctx;

pub(super) struct FakeClock(parking_lot::Mutex<(Duration, SystemTime)>);

impl FakeClock {
    pub(super) fn new() -> Self {
        Self(parking_lot::Mutex::new((
            Duration::from_secs(10_000),
            SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000),
        )))
    }

    fn advance(&self, elapsed: u64, wall_jump: i64) {
        let mut time = self.0.lock();
        time.0 += Duration::from_secs(elapsed);
        time.1 += Duration::from_secs(elapsed);
        if wall_jump >= 0 {
            time.1 += Duration::from_secs(wall_jump as u64);
        } else {
            time.1 -= Duration::from_secs(wall_jump.unsigned_abs());
        }
    }
}

impl RouteClock for FakeClock {
    fn instant(&self) -> Duration {
        self.0.lock().0
    }
    fn wall_time(&self) -> SystemTime {
        self.0.lock().1
    }
}

fn service(clock: &Arc<FakeClock>) -> PeerRouteServiceImpl {
    let mut service = PeerRouteServiceImpl::new(1, get_mock_global_ctx());
    service.synced_route_info.clock = clock.clone();
    service
}

fn receive_peer(
    service: &PeerRouteServiceImpl,
    peer_id: PeerId,
    version: Version,
) -> RoutePeerInfo {
    let info = RoutePeerInfo {
        peer_id,
        version,
        // Deliberately unrelated sender wall time, as on an unpatched node.
        last_update: Some(SystemTime::UNIX_EPOCH.into()),
        ..RoutePeerInfo::new()
    };
    let raw = DynamicMessage::decode(info.descriptor(), info.encode_to_vec().as_slice()).unwrap();
    service
        .synced_route_info
        .update_peer_infos(
            1,
            service.my_peer_route_id,
            peer_id,
            &[info.clone()],
            &[raw],
        )
        .unwrap();
    info
}

fn connect(service: &PeerRouteServiceImpl) {
    service.synced_route_info.update_my_peer_info(
        1,
        service.my_peer_route_id,
        &service.global_ctx,
        None,
    );
    service
        .synced_route_info
        .update_my_conn_info(1, BTreeSet::from([2]));
    service.synced_route_info.update_conn_info_one_peer(
        &PeerIdVersion {
            peer_id: 2,
            version: 1,
        },
        BTreeSet::from([1]),
    );
    service.update_route_table();
    assert!(service.route_table.peer_reachable(2));
}

fn foreign(
    key: ForeignNetworkRouteInfoKey,
    wall: SystemTime,
    version: Version,
) -> RouteForeignNetworkInfos {
    RouteForeignNetworkInfos {
        infos: vec![route_foreign_network_infos::Info {
            key: Some(key),
            value: Some(ForeignNetworkRouteInfoEntry {
                foreign_peer_ids: vec![20],
                last_update: Some(wall.into()),
                version,
                ..Default::default()
            }),
        }],
    }
}

#[tokio::test]
async fn wall_jumps_do_not_expire_peer_or_foreign_routes() {
    for jump in [1200, 1245, -1200, -86400] {
        let clock = Arc::new(FakeClock::new());
        let service = service(&clock);
        receive_peer(&service, 2, 1);
        connect(&service);
        let key = ForeignNetworkRouteInfoKey {
            peer_id: 2,
            network_name: "clock-test".into(),
        };
        service.synced_route_info.update_foreign_network(&foreign(
            key.clone(),
            clock.wall_time(),
            1,
        ));
        clock.advance(2700, jump);
        service.clear_expired_peer().await;
        assert!(
            service.synced_route_info.peer_infos.read().contains_key(&2),
            "jump={jump}"
        );
        assert!(
            service.synced_route_info.foreign_network.contains_key(&key),
            "jump={jump}"
        );
        // The wire timestamp still records the actual local wall time at receipt.
        let stored = service
            .synced_route_info
            .peer_infos
            .read()
            .get(&2)
            .unwrap()
            .last_update
            .unwrap();
        let expected = SystemTime::UNIX_EPOCH + Duration::from_secs(1_800_000_000);
        assert_eq!(SystemTime::try_from(stored).unwrap(), expected);
    }
}

#[tokio::test]
async fn suspend_two_hours_expires_cached_routes() {
    let clock = Arc::new(FakeClock::new());
    let service = service(&clock);
    receive_peer(&service, 2, 1);
    connect(&service);
    let key = ForeignNetworkRouteInfoKey {
        peer_id: 2,
        network_name: "suspend-test".into(),
    };
    service
        .synced_route_info
        .update_foreign_network(&foreign(key.clone(), clock.wall_time(), 1));
    clock.advance(7200, 0);
    service.clear_expired_peer().await;
    assert!(!service.synced_route_info.peer_infos.read().contains_key(&2));
    assert!(!service.synced_route_info.foreign_network.contains_key(&key));
    assert!(!service.synced_route_info.peer_updated_at.contains_key(&2));
    assert!(
        !service
            .synced_route_info
            .foreign_updated_at
            .contains_key(&key)
    );
}

#[tokio::test]
async fn unreachable_cleanup_keeps_ninety_second_boundary() {
    let clock = Arc::new(FakeClock::new());
    let service = service(&clock);
    receive_peer(&service, 2, 1);
    clock.advance(89, -1200);
    service.clear_expired_peer().await;
    assert!(service.synced_route_info.peer_infos.read().contains_key(&2));
    clock.advance(2, 1200);
    service.clear_expired_peer().await;
    assert!(!service.synced_route_info.peer_infos.read().contains_key(&2));
}

#[tokio::test]
async fn incremental_sync_survives_wall_jumps_and_does_not_repeat_acknowledged_updates() {
    for jump in [1200, -1200, -86400] {
        let clock = Arc::new(FakeClock::new());
        let service = service(&clock);
        receive_peer(&service, 2, 1);
        connect(&service);
        let session = service.get_or_create_session(3);
        let first = service.build_route_info(&session).unwrap();
        session.update_dst_saved_peer_info_version(&first, 3);
        let first_conn = service.build_conn_peer_list(&session, &mut 0).unwrap();
        session.update_dst_saved_conn_peer_list_version(&first_conn, 3);
        session.update_last_sync_succ_timestamp(
            service
                .synced_route_info
                .get_next_last_sync_succ_timestamp(),
        );
        clock.advance(1, jump);
        receive_peer(&service, 2, 2);
        service.synced_route_info.update_conn_info_one_peer(
            &PeerIdVersion {
                peer_id: 2,
                version: 2,
            },
            BTreeSet::from([1]),
        );
        let update = service.build_route_info(&session).unwrap();
        assert_eq!(update.len(), 1);
        assert_eq!((update[0].peer_id, update[0].version), (2, 2));
        assert_eq!(
            SystemTime::try_from(update[0].last_update.unwrap()).unwrap(),
            clock.wall_time()
        );
        let conns = service.build_conn_peer_list(&session, &mut 0).unwrap();
        assert_eq!(conns.peer_conn_infos.len(), 1);
        assert_eq!(
            conns.peer_conn_infos[0].peer_id.as_ref().unwrap().version,
            2
        );
        // Retry before acknowledgement, then stop after acknowledgement.
        assert_eq!(service.build_route_info(&session).unwrap(), update);
        session.update_dst_saved_peer_info_version(&update, 3);
        session.update_dst_saved_conn_peer_list_version(&conns, 3);
        session.update_last_sync_succ_timestamp(
            service
                .synced_route_info
                .get_next_last_sync_succ_timestamp(),
        );
        assert!(service.build_route_info(&session).is_none());
        assert!(service.build_conn_peer_list(&session, &mut 0).is_none());
    }
}

#[tokio::test]
async fn periodic_self_refresh_uses_elapsed_time() {
    for jump in [1200, -1200] {
        let clock = Arc::new(FakeClock::new());
        let service = service(&clock);
        let update = || {
            service.synced_route_info.update_my_peer_info(
                1,
                service.my_peer_route_id,
                &service.global_ctx,
                None,
            )
        };
        assert!(update());
        clock.advance(2700, jump);
        assert!(!update());
        clock.advance(901, 0);
        assert!(update());
        assert!(!update());
    }
}

#[tokio::test]
async fn foreign_versions_increase_within_a_process_after_wall_rollback() {
    let clock = Arc::new(FakeClock::new());
    let service = service(&clock);
    let key = ForeignNetworkRouteInfoKey {
        peer_id: 1,
        network_name: "own-test".into(),
    };
    let update = |peer_id| {
        let map = ForeignNetworkRouteInfoMap::new();
        map.insert(
            key.clone(),
            ForeignNetworkRouteInfoEntry {
                foreign_peer_ids: vec![peer_id],
                last_update: Some(clock.wall_time().into()),
                ..Default::default()
            },
        );
        service.synced_route_info.update_my_foreign_network(1, map)
    };
    assert!(update(20));
    let before = service
        .synced_route_info
        .foreign_network
        .get(&key)
        .unwrap()
        .version;
    clock.advance(1, -1200);
    assert!(update(21));
    let after = service
        .synced_route_info
        .foreign_network
        .get(&key)
        .unwrap()
        .version;
    assert!(after > before);
    assert!(!update(21));
    clock.advance(3601, 0);
    assert!(update(21));
    assert!(
        service
            .synced_route_info
            .foreign_network
            .get(&key)
            .unwrap()
            .version
            > after
    );
}

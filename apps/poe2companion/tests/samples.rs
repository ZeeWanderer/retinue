use std::collections::BTreeMap;

use poe2companion::{Observation, Sample, Status};
use retinue_client::reload::Checkpoint;
use retinue_poe2::{
    process::Process,
    reader::{Area, Entities, Identity, Issue, Player, Snapshot, Social, UniqueMonster},
};
use serde_json::json;

#[test]
fn native_sample_preserves_process_snapshot_identity_and_unavailable_fields() {
    let process = Process {
        pid: 42,
        started: 100,
        executable: "/game/Path of Exile 2/PathOfExile.exe".into(),
        image_base: 0x140000000,
    };
    let snapshot = Snapshot {
        scene: "game".into(),
        states: vec!["InGameState".into()],
        player: Some(Player {
            name: "Test".into(),
            class: Some("Huntress".into()),
            level: 2,
            experience: 17,
            gold: None,
            life: Some(23),
        }),
        area: Some(Area {
            name: "Test zone".into(),
            in_town: false,
            change_key: 9,
        }),
        panels: BTreeMap::from([("inventory".into(), Some(false)), ("stash".into(), None)]),
        social: Some(Social {
            local: Some(Identity {
                character: "Test".into(),
                account: "Example#1234".into(),
                league: "Standard".into(),
            }),
            party: vec![],
        }),
        entities: Some(Entities {
            tree: 0x100000,
            area_key: 9,
            count: 2,
            unique_monsters: vec![UniqueMonster {
                id: 1,
                address: 0x110000,
                name: "Example Unique".into(),
                life: 100,
            }],
        }),
        issues: vec![Issue {
            field: "stash".into(),
            reason: "unavailable".into(),
        }],
    };
    let observation = Observation::from_result(Ok(Some((process.clone(), snapshot.clone()))));
    assert_eq!(observation.status, Status::Observed);
    let sample = Sample {
        stream: "test-stream".into(),
        sequence: 7,
        observed_at: 1234,
        observation: observation.clone(),
    };
    let encoded = serde_json::to_value(&sample).unwrap();
    assert_eq!(encoded["status"], "observed");
    assert!(encoded.get("observation").is_none());
    assert_eq!(encoded["snapshot"]["panels"]["inventory"], false);
    assert!(encoded["snapshot"]["panels"]["stash"].is_null());
    assert!(encoded["snapshot"]["player"]["gold"].is_null());
    assert_eq!(encoded["snapshot"]["social"]["local"]["league"], "Standard");
    assert_eq!(
        encoded["snapshot"]["entities"]["unique_monsters"][0]["life"],
        100
    );
    let restored: Sample = serde_json::from_value(encoded).unwrap();
    assert_eq!(restored.stream, sample.stream);
    assert_eq!(restored.sequence, 7);
    assert_eq!(restored.observed_at, 1234);
    assert_eq!(restored.observation, observation);
    assert_eq!(restored.observation.process, Some(process));
    assert_eq!(restored.observation.snapshot, Some(snapshot));
}

#[test]
fn missing_process_and_failed_read_have_distinct_statuses_without_stale_data() {
    let waiting = Observation::from_result(Ok(None));
    assert_eq!(waiting, Observation::default());
    assert_eq!(serde_json::to_value(waiting).unwrap()["status"], "waiting");
    let error = Observation::from_result(Err("native read failed".into()));
    assert_eq!(error.status, Status::Error);
    assert_eq!(error.error.as_deref(), Some("native read failed"));
    assert!(error.process.is_none());
    assert!(error.snapshot.is_none());
}

#[test]
fn checkpoint_validates_pending_identity_bounds_and_ordering() {
    let mut saved = serde_json::to_value(poe2companion::runtime::State::default()).unwrap();
    let stream = saved["stream"].clone();
    saved["sequence"] = json!(2);
    let sample = |sequence| {
        json!({"stream":stream,"sequence":sequence,"observed_at":1234,
        "status":"waiting","process":null,"snapshot":null,"error":null})
    };
    saved["pending"] = sample(1);
    saved["queued"] = sample(2);
    let restored: poe2companion::runtime::State = serde_json::from_value(saved.clone()).unwrap();
    restored.validate().unwrap();
    assert_eq!(serde_json::to_value(restored).unwrap(), saved);
    for (field, value) in [
        ("stream", json!("other")),
        ("sequence", json!(0)),
        ("sequence", json!(3)),
    ] {
        let mut invalid = saved.clone();
        invalid["pending"][field] = value;
        assert!(
            serde_json::from_value::<poe2companion::runtime::State>(invalid)
                .unwrap()
                .validate()
                .is_err()
        );
    }
    let mut invalid = saved.clone();
    invalid["queued"] = sample(1);
    assert!(
        serde_json::from_value::<poe2companion::runtime::State>(invalid)
            .unwrap()
            .validate()
            .is_err()
    );
    saved["pending"]["error"] = json!("x".repeat(65536));
    assert!(
        serde_json::from_value::<poe2companion::runtime::State>(saved)
            .unwrap()
            .validate()
            .is_err()
    );
}

use retinue_protocol::{
    prost::{Message, Name},
    warframe::*,
};

#[test]
fn presence_and_large_identifiers_survive_wire() {
    for value in [None, Some(false), Some(true)] {
        let slot = Slot {
            unlocked: value,
            ..Default::default()
        };
        assert_eq!(Slot::decode(slot.encode_to_vec().as_slice()).unwrap(), slot);
    }
    for value in [None, Some(0), Some(u64::MAX)] {
        let profile = Profile {
            player_level: value,
            player_name: "\u{03a9}".into(),
            ..Default::default()
        };
        assert_eq!(
            Profile::decode(profile.encode_to_vec().as_slice()).unwrap(),
            profile
        );
    }
    assert_eq!(Equipment::full_name(), "retinue.warframe.v1.Equipment");
}

#[test]
fn unknown_fields_are_ignored_but_truncation_is_not_empty_data() {
    let data = Equipment {
        player_revision: 9,
        ..Default::default()
    };
    let mut bytes = data.encode_to_vec();
    bytes.extend_from_slice(&[0xa0, 0x06, 0x01]);
    assert_eq!(Equipment::decode(bytes.as_slice()).unwrap(), data);
    assert!(Equipment::decode(&[0x12, 0xff][..]).is_err());
}

#[test]
#[ignore = "requires private canonical snapshots and Erlang-produced wire files"]
fn supplied_real_views_match_erlang_projection() {
    let root = std::env::var("RETINUE_VIEW_FIXTURES").expect("set RETINUE_VIEW_FIXTURES");
    fn check<T: Message + Default + PartialEq + std::fmt::Debug + serde::de::DeserializeOwned>(
        root: &str,
        name: &str,
    ) {
        let bytes = std::fs::read(format!("{root}/native-{name}.pb")).unwrap();
        let json = std::fs::read(format!("{root}/payload-{name}.json")).unwrap();
        let expected: T = serde_json::from_slice(&json).unwrap();
        let actual = T::decode(bytes.as_slice()).unwrap();
        assert!(
            actual == expected,
            "{name}: wire and canonical fields differ"
        );
        std::fs::write(
            format!("{root}/native-rust-{name}.pb"),
            actual.encode_to_vec(),
        )
        .unwrap();
    }
    check::<FoundrySnapshot>(&root, "foundry");
    for name in ["inventory", "mastery", "relics"] {
        check::<ListingSnapshot>(&root, name);
    }
    check::<Equipment>(&root, "equipment");
}

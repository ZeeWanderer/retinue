pub(super) fn scene() -> crate::relic::Scene {
    crate::relic::Scene::Rewards(crate::relic::Rewards {
        items: vec![
            crate::relic::Reward {
                name: "Lex Prime Barrel".to_owned(),
                slug: Some("lex_prime_barrel".to_owned()),
                game_ref: None,
                ducats: Some(15),
                lowest_sell: Some(15),
                highest_buy: Some(11),
                count_owned: 2,
                total_to_own: 2,
                crafted: Some(false),
                set_complete: Some(false),
                vaulted: true,
                set_price: Some(78),
                asset: None,
                parts: parts(
                    &[("Blueprint", 1, 1), ("Barrel", 2, 2), ("Receiver", 0, 1)],
                    1,
                ),
            },
            crate::relic::Reward {
                name: "Gara Prime Systems Blueprint".to_owned(),
                slug: Some("gara_prime_systems_blueprint".to_owned()),
                game_ref: None,
                ducats: Some(100),
                lowest_sell: Some(8),
                highest_buy: Some(6),
                count_owned: 1,
                total_to_own: 1,
                crafted: Some(true),
                set_complete: Some(true),
                vaulted: false,
                set_price: Some(42),
                asset: None,
                parts: parts(
                    &[
                        ("Blueprint", 1, 1),
                        ("Chassis", 1, 1),
                        ("Neuroptics", 1, 1),
                        ("Systems", 1, 1),
                    ],
                    3,
                ),
            },
            crate::relic::Reward {
                name: "Inaros Prime Systems Blueprint".to_owned(),
                slug: Some("inaros_prime_systems_blueprint".to_owned()),
                game_ref: None,
                ducats: Some(100),
                lowest_sell: Some(12),
                highest_buy: Some(9),
                count_owned: 0,
                total_to_own: 1,
                crafted: Some(false),
                set_complete: Some(false),
                vaulted: true,
                set_price: Some(51),
                asset: None,
                parts: parts(
                    &[
                        ("Blueprint", 1, 1),
                        ("Chassis", 0, 1),
                        ("Neuroptics", 2, 1),
                        ("Systems", 0, 1),
                    ],
                    3,
                ),
            },
            crate::relic::Reward {
                name: "Forma Blueprint".to_owned(),
                slug: None,
                game_ref: Some("/Lotus/Types/Recipes/Components/FormaBlueprint".to_owned()),
                ducats: Some(0),
                lowest_sell: Some(2),
                highest_buy: None,
                count_owned: 4,
                total_to_own: 1,
                crafted: Some(true),
                set_complete: None,
                vaulted: false,
                set_price: None,
                asset: None,
                parts: Vec::new(),
            },
        ],
        account: crate::relic::Account {
            platinum: Some(124),
            ducats: Some(915),
        },
    })
}

fn parts(parts: &[(&str, u64, u64)], current: usize) -> Vec<crate::relic::SetPart> {
    parts
        .iter()
        .enumerate()
        .map(|(index, (name, owned, required))| crate::relic::SetPart {
            name: (*name).to_owned(),
            owned: *owned,
            required: *required,
            current: index == current,
            asset: part_asset(name),
        })
        .collect()
}

pub(super) fn part_asset(name: &str) -> Option<crate::relic::Asset> {
    let id = match name {
        "Blueprint" => "sub_icons/blueprint_128x128.png",
        "Barrel" => "sub_icons/weapon/prime_barrel_128x128.png",
        "Receiver" => "sub_icons/weapon/prime_receiver_128x128.png",
        "Chassis" => "sub_icons/warframe/prime_chassis_128x128.png",
        "Neuroptics" => "sub_icons/sentinel/prime_cerebrum_128x128.png",
        "Systems" => "sub_icons/sentinel/prime_systems_128x128.png",
        _ => return None,
    };
    Some(crate::relic::Asset {
        id: format!("preview:{name}"),
        source: "fixture".to_owned(),
        image_name: id.to_owned(),
        path: format!(
            "{}/test/fixtures/relic-parts/{id}",
            env!("CARGO_MANIFEST_DIR")
        ),
        digest: id.to_owned(),
    })
}

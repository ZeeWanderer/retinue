use super::*;
use crate::domain::trade::validate_item_text;
use serde_json::{Value, json};

#[test]
fn currency_request_tracks_results_estimate_scale_and_generation() {
    let mut model = EvaluateModel {
        generation: 4,
        results: Some(Results {
            listings: vec![Listing {
                currency: "exalted".into(),
                amount: 12.0,
                ..Default::default()
            }],
            ..Default::default()
        }),
        estimate: Some(Estimate {
            currency: "divine".into(),
            amount: 2.0,
            uncertainty: None,
        }),
        ..Default::default()
    };
    let request = model.currency_request(1.25);
    assert_eq!(
        request,
        crate::domain::currency::Request::new(["exalted".into(), "divine".into()], 25, 4)
    );
    assert_eq!(model.results.as_ref().unwrap().listings[0].amount, 12.0);
    model.generation += 1;
    assert_ne!(request, model.currency_request(1.25));
}

fn paged_model() -> EvaluateModel {
    let mut m = EvaluateModel::default();
    m.searched(
        0,
        Results {
            search_id: "search".into(),
            returned: 25,
            next_offset: Some(0),
            ..Results::default()
        },
    );
    m
}

fn page(offset: u32, end: u32, next: Option<u32>) -> ListingPage {
    ListingPage {
        search_id: "search".into(),
        offset,
        next_offset: next,
        listings: (offset..end)
            .map(|i| Listing {
                id: i.to_string(),
                ..Listing::default()
            })
            .collect(),
    }
}

#[test]
fn pages_follow_visible_threshold_and_preserve_server_order() {
    let mut m = paged_model();
    assert_eq!(m.fetch(0, 8).unwrap()["offset"], 0);
    assert!(m.fetch(0, 8).is_none());
    m.fetched(0, page(0, 10, Some(10)));
    assert_eq!(m.fetch(0, 8).unwrap()["offset"], 10);
    m.fetched(0, page(10, 20, Some(20)));
    assert!(m.fetch(0, 8).is_none());
    assert!(m.fetch(9, 8).is_none());
    assert_eq!(m.fetch(10, 8).unwrap()["offset"], 20);
    m.fetched(0, page(20, 25, None));
    assert!(m.fetch(20, 8).is_none());
    assert_eq!(
        m.results
            .unwrap()
            .listings
            .iter()
            .map(|l| l.id.clone())
            .collect::<Vec<_>>(),
        (0..25).map(|i| i.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn deleted_rows_advance_and_duplicate_or_stale_replies_do_not_append() {
    let mut m = paged_model();
    m.fetch(0, 8).unwrap();
    m.fetched(0, page(0, 0, Some(10)));
    assert_eq!(m.fetch(0, 8).unwrap()["offset"], 10);
    m.fetched(0, page(0, 10, Some(10)));
    assert!(m.fetching);
    assert!(m.results.as_ref().unwrap().listings.is_empty());
    m.fetched(0, page(10, 20, Some(20)));
    m.fetched(0, page(10, 20, Some(20)));
    assert_eq!(m.results.as_ref().unwrap().listings.len(), 10);
    m.invalidate();
    m.fetched(0, page(20, 25, None));
    assert!(m.results.is_none());
    assert!(!m.fetching);
}

#[test]
fn failed_fetch_keeps_rows_without_automatic_retry_or_search_error_plaque() {
    let mut m = paged_model();
    m.fetch(0, 8).unwrap();
    m.fetched(0, page(0, 10, Some(10)));
    m.fetch(0, 8).unwrap();
    m.failed(0, "rate limited".into());
    m.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(30));
    assert_eq!(m.results.as_ref().unwrap().listings.len(), 10);
    assert_eq!(m.results.as_ref().unwrap().next_offset, Some(10));
    assert!(m.error.is_none());
    assert_eq!(m.fetch_error.as_deref(), Some("rate limited"));
    assert_eq!(m.retry_after(), 30);
    assert!(m.tick_cooldown());
    assert!(!m.tick_cooldown());
    assert!(m.fetch(9, 8).is_none());
    m.retry_at = Some(std::time::Instant::now());
    assert!(m.tick_cooldown());
    assert!(m.fetch(9, 8).is_none());
    m.invalidate();
    assert!(m.fetch_error.is_none());
}

#[test]
fn cooldown_wakes_at_display_boundary_and_stops_when_finished() {
    let now = std::time::Instant::now();
    let mut model = EvaluateModel::default();
    assert_eq!(model.cooldown_deadline(now), None);
    model.retry_at = Some(now + std::time::Duration::from_millis(2500));
    model.cooldown_display = 3;
    assert_eq!(
        model.cooldown_deadline(now),
        Some(now + std::time::Duration::from_millis(500))
    );
    let late = now + std::time::Duration::from_secs(4);
    assert_eq!(model.cooldown_deadline(late), Some(late));
    model.cooldown_display = 0;
    assert_eq!(model.cooldown_deadline(late), None);
}

#[test]
fn prediction_survives_search_edits_but_not_a_new_item_league_or_close() {
    let mut m = EvaluateModel::default();
    m.league = "A".into();
    m.evaluation = Some(Evaluation {
        prediction_available: true,
        ..Evaluation::default()
    });
    let (first, request) = m.predict().unwrap();
    assert_eq!(request["league"], "A");
    m.invalidate();
    let estimate = || {
        Some(Estimate {
            amount: 40.0,
            currency: "exalted".into(),
            uncertainty: None,
        })
    };
    m.predicted(first, estimate());
    assert_eq!(m.estimate.as_ref().unwrap().amount, 40.0);
    m.failed(m.generation, "search failed".into());
    assert!(m.estimate.is_some());
    m.league = "B".into();
    let (second, _) = m.predict().unwrap();
    m.predicted(first, estimate());
    assert!(m.predicting);
    assert!(m.estimate.is_none());
    m.predicted(second, None);
    assert!(!m.predicting);
    assert_eq!(m.error.as_deref(), Some("search failed"));
    let (third, _) = m.predict().unwrap();
    m.copy();
    m.predicted(third, estimate());
    assert!(m.estimate.is_none());
    m.evaluation = Some(Evaluation {
        prediction_available: true,
        ..Evaluation::default()
    });
    let (last, _) = m.predict().unwrap();
    m.dismiss();
    m.predicted(last, estimate());
    assert!(m.estimate.is_none());
}
fn fixture() -> EvaluateModel {
    let data: Value = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/evaluate-reference.json"
    )))
    .unwrap();
    let mut model = EvaluateModel::new(String::new());
    model.evaluated(
        0,
        serde_json::from_value(data["evaluation"].clone()).unwrap(),
    );
    model
}

#[test]
fn profile_selection_survives_filter_edits_and_searches() {
    let mut model = fixture();
    assert_eq!(model.profile, Some(Profile::Quick));
    model
        .evaluation
        .as_mut()
        .unwrap()
        .item
        .profiles
        .push(Profile::Exact);
    model.apply_profile(Profile::Exact);
    model.invalidate();
    assert_eq!(model.profile, Some(Profile::Exact));
    assert!(model.filters.iter().all(|filter| filter.enabled));
    model.filters[0].enabled = false;
    model.filters[1].min = "30".into();
    model.invalidate();
    assert_eq!(model.profile, Some(Profile::Exact));
    model.apply_profile(Profile::Broad);
    model.invalidate();
    assert_eq!(model.profile, Some(Profile::Broad));
    assert!(!model.filters[0].enabled);
    assert!(model.filters[1].enabled);
    model.search().unwrap();
    assert_eq!(model.profile, Some(Profile::Broad));
    model.searched(model.generation, Results::default());
    assert_eq!(model.profile, Some(Profile::Broad));
    model.copy();
    assert_eq!(model.profile, None);
}

#[test]
fn refresh_preserves_rows_until_search_returns_and_clears_them_on_filter_edit() {
    let mut m = fixture();
    let old = Results {
        search_id: "old".into(),
        listings: page(0, 3, None).listings,
        ..Results::default()
    };
    m.results = Some(old);
    let request = m.search().unwrap();
    assert_eq!(request["refresh"], true);
    assert!(m.loading);
    assert_eq!(m.results.as_ref().unwrap().listings.len(), 3);
    m.failed(m.generation, "upstream unavailable".into());
    assert!(m.error.is_none());
    assert_eq!(m.results.as_ref().unwrap().listings.len(), 3);
    m.search().unwrap();
    m.searched(
        m.generation,
        Results {
            search_id: "new".into(),
            next_offset: Some(0),
            ..Results::default()
        },
    );
    assert_eq!(m.results.as_ref().unwrap().search_id, "new");
    assert!(m.results.as_ref().unwrap().listings.is_empty());
    m.invalidate();
    assert!(m.results.is_none());
    assert_eq!(m.search().unwrap()["refresh"], true);
}

#[test]
fn exact_profile_keeps_merged_modifiers_and_unranked_properties_off() {
    let mut model = fixture();
    let item = &mut model.evaluation.as_mut().unwrap().item;
    item.profiles.push(Profile::Exact);
    item.modifiers[0].section = "property".into();
    item.modifiers[0].quick_selected = true;
    item.modifiers[0].exact_selected = true;
    item.modifiers[1].exact_selected = false;
    item.modifiers[2].section = "property".into();
    item.modifiers[2].exact_selected = false;
    model.apply_profile(Profile::Exact);
    assert_eq!(
        model.filters.iter().map(|f| f.enabled).collect::<Vec<_>>(),
        vec![true, false, false]
    );
    model.apply_profile(Profile::Broad);
    assert_eq!(
        model.filters.iter().map(|f| f.enabled).collect::<Vec<_>>(),
        vec![true, false, false]
    );
}

#[test]
fn profile_is_not_inferred_from_manually_selected_filters() {
    let mut model = fixture();
    for filter in &mut model.filters {
        filter.enabled = true;
    }
    model.invalidate();
    assert_eq!(model.profile, Some(Profile::Quick));
    model.apply_profile(Profile::Broad);
    for filter in &mut model.filters {
        filter.enabled = false;
    }
    model.invalidate();
    assert_eq!(model.profile, Some(Profile::Broad));
    let evaluation = model.evaluation.clone().unwrap();
    model.evaluated(model.generation.wrapping_sub(1), evaluation.clone());
    assert_eq!(model.profile, Some(Profile::Broad));
    model.evaluated(model.generation, evaluation);
    assert_eq!(model.profile, Some(Profile::Quick));
}

#[test]
fn profile_bounds_are_supplied_by_daemon_for_every_row() {
    let cases: Vec<Value> = serde_json::from_slice(include_bytes!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test/fixtures/poe2_trade_profiles.json"
    )))
    .unwrap();
    for case in cases {
        let mut model = fixture();
        let mut row = case["modifier"].clone();
        row["key"] = json!("profile-test");
        row["text"] = case["name"].clone();
        row["supported"] = json!(true);
        row["exact_selected"] = json!(true);
        row["ranges"] = case["expected"].clone();
        let mut evaluation = model.evaluation.take().unwrap();
        evaluation.item.profiles.push(Profile::Exact);
        evaluation.item.modifiers = vec![serde_json::from_value(row).unwrap()];
        model.evaluated(0, evaluation);
        let expected = |profile: &str, bound: &str| {
            case["expected"][profile][bound]
                .as_f64()
                .map(number)
                .unwrap_or_default()
        };
        assert_eq!(model.filters[0].min, expected("allmods", "min"));
        assert_eq!(model.filters[0].max, expected("allmods", "max"));
        for enabled in [false, true] {
            model.filters[0] = Filter {
                enabled,
                min: "999".into(),
                max: "999".into(),
            };
            for _ in 0..2 {
                model.apply_profile(Profile::Broad);
                assert_eq!(model.filters[0].enabled, enabled);
                assert_eq!(model.filters[0].min, expected("fuzzy", "min"));
                assert_eq!(model.filters[0].max, expected("fuzzy", "max"));
            }
        }
        model.apply_profile(Profile::Exact);
        assert!(model.filters[0].enabled);
        assert_eq!(model.filters[0].min, expected("allmods", "min"));
        assert_eq!(model.filters[0].max, expected("allmods", "max"));
        let query = model.search().unwrap();
        for bound in ["min", "max"] {
            assert_eq!(
                query["filters"][0][bound].as_f64(),
                case["expected"]["allmods"][bound].as_f64()
            );
        }
    }
}

#[test]
fn category_defaults_and_profile_inclusion_come_from_daemon() {
    let mut model = fixture();
    assert_eq!(model.profile, Some(Profile::Quick));
    assert!(!model.base);
    assert!(!model.level.enabled);
    assert!(model.filters.iter().all(|f| !f.enabled));
    model.apply_profile(Profile::Exact);
    assert_eq!(model.profile, Some(Profile::Quick));

    let mut evaluation = model.evaluation.take().unwrap();
    evaluation.item.profiles = vec![Profile::Base, Profile::Quick, Profile::Broad];
    evaluation.item.default_profile = Some(Profile::Base);
    evaluation.item.modifiers[0].base_selected = true;
    evaluation.item.modifiers[1].quick_selected = true;
    model.evaluated(model.generation, evaluation);
    assert_eq!(model.profile, Some(Profile::Base));
    assert!(model.base && model.level.enabled);
    assert_eq!(model.level.min, "82");
    assert_eq!(
        model.filters.iter().map(|f| f.enabled).collect::<Vec<_>>(),
        [true, false, false]
    );
    model.apply_profile(Profile::Quick);
    assert!(!model.base && !model.level.enabled);
    assert_eq!(
        model.filters.iter().map(|f| f.enabled).collect::<Vec<_>>(),
        [false, true, false]
    );
    model.apply_profile(Profile::Broad);
    assert!(model.filters[1].enabled);
}

#[test]
fn query_options_match_metadata_and_disabled_fields_stay_out() {
    let mut model = fixture();
    let query = model.search().unwrap();
    assert_eq!(query["status"], "securable");
    assert_eq!(query["rarity"], "rare");
    assert_eq!(query["sort"], "asc");
    assert_eq!(query["conditions"], json!(["modifiable"]));
    assert!(query.get("item_level").is_none());
    assert!(query["currency"].is_null());
    assert!(!query.as_object().unwrap().contains_key("online"));
    model.loading = false;
    model.status = "any".into();
    model.currency = Some("divine".into());
    model.indexed = Some("1day".into());
    model.rarity = None;
    model.descending = true;
    model.conditions[0] = false;
    model.level.enabled = true;
    model.level.min = "70".into();
    model.level.max = "85".into();
    let query = model.search().unwrap();
    assert_eq!(query["status"], "any");
    assert!(query["rarity"].is_null());
    assert_eq!(query["currency"], "divine");
    assert_eq!(query["indexed"], "1day");
    assert_eq!(query["conditions"], json!([]));
    assert_eq!(query["sort"], "desc");
    assert_eq!(query["item_level"], json!({"min":70,"max":85}));
}

#[test]
fn invalid_level_does_not_start_or_discard_a_search() {
    let mut model = fixture();
    model.results = Some(Results::default());
    model.level.enabled = true;
    for value in ["NaN", "0", "101", "40.5"] {
        model.level.min = value.into();
        assert!(model.search().is_err());
        assert!(!model.loading);
        assert!(model.results.is_some());
        assert_eq!(model.generation, 0);
    }
}

#[test]
fn automatic_search_uses_daemon_eligibility_and_budget() {
    for eligible in [false, true] {
        for allowed in [false, true] {
            let mut model = EvaluateModel::default();
            model.evaluation = Some(Evaluation {
                item: Item {
                    auto_search: eligible,
                    ..Item::default()
                },
                ..Evaluation::default()
            });
            assert_eq!(
                model.restored(
                    0,
                    Restoration {
                        results: None,
                        auto_search_allowed: allowed
                    },
                    false
                ),
                eligible && allowed
            );
        }
    }
}

#[test]
fn cache_lookup_can_restore_rare_items_during_cooldown_without_searching() {
    let mut model = fixture();
    model.retry_at = Some(std::time::Instant::now() + std::time::Duration::from_secs(60));
    let request = model.restore().unwrap();
    assert_eq!(request["op"], "trade_restore");
    assert!(request.get("refresh").is_none());
    assert!(!model.loading);
    let restored = Restoration {
        results: Some(Results {
            search_id: "cached".into(),
            listings: page(0, 3, Some(10)).listings,
            next_offset: Some(10),
            ..Results::default()
        }),
        auto_search_allowed: false,
    };
    assert!(!model.restored(model.generation, restored, false));
    assert_eq!(model.results.as_ref().unwrap().listings.len(), 3);
    assert_eq!(model.results.as_ref().unwrap().next_offset, Some(10));
    assert!(!model.loading);
    assert!(model.search().is_err());
}

#[test]
fn cache_miss_tracks_only_the_last_automatic_attempt() {
    let mut model = fixture();
    model.evaluation.as_mut().unwrap().item.auto_search = true;
    let miss = || Restoration {
        results: None,
        auto_search_allowed: true,
    };
    model.text = "first".into();
    assert!(model.restored(model.generation, miss(), false));
    model.search().unwrap();
    model.failed(model.generation, "upstream failed".into());
    model.restore().unwrap();
    assert!(!model.restored(model.generation, miss(), false));
    model.text = "second".into();
    model.restore().unwrap();
    assert!(model.restored(model.generation, miss(), false));
    model.text = "first".into();
    model.restore().unwrap();
    assert!(model.restored(model.generation, miss(), false));
    model.restore().unwrap();
    assert!(model.restored(model.generation, miss(), true));
}

#[test]
fn cache_miss_on_a_rare_item_waits_for_search_or_a_result_selector() {
    let mut model = fixture();
    model.evaluation.as_mut().unwrap().item.auto_search = false;
    let miss = || Restoration {
        results: None,
        auto_search_allowed: true,
    };
    model.restore().unwrap();
    assert!(!model.restored(model.generation, miss(), false));
    assert!(model.results.is_none());
    assert!(!model.loading);
    assert!(model.restored(model.generation, miss(), true));
}

#[test]
fn automatic_budget_denial_is_not_retried_but_empty_cached_results_are_hits() {
    let mut model = fixture();
    model.evaluation.as_mut().unwrap().item.auto_search = true;
    let reply = |allowed| Restoration {
        results: None,
        auto_search_allowed: allowed,
    };
    assert!(!model.restored(model.generation, reply(false), false));
    assert!(!model.restored(model.generation, reply(true), false));
    assert!(model.restored(model.generation, reply(true), true));
    let empty: Restoration = serde_json::from_value(json!({
        "auto_search_allowed": true,
        "results": {"search_id":"empty", "url":"", "total":0, "returned":0,
            "listings":[], "next_offset":null}
    }))
    .unwrap();
    assert!(!model.restored(model.generation, empty, true));
    assert_eq!(model.results.as_ref().unwrap().total, 0);
}

#[test]
fn late_cache_lookup_cannot_replace_a_manual_search_or_edited_query() {
    let mut model = fixture();
    let hit = || Restoration {
        results: Some(Results::default()),
        auto_search_allowed: true,
    };
    model.restore().unwrap();
    let old = model.generation;
    assert_eq!(model.search().unwrap()["refresh"], true);
    assert!(!model.restored(old, hit(), false));
    assert!(model.results.is_none());
    assert!(model.loading);
    model.invalidate();
    model.restore().unwrap();
    let old = model.generation;
    model.filters[0].enabled = !model.filters[0].enabled;
    model.invalidate();
    model.restore().unwrap();
    assert!(!model.restored(old, hit(), false));
    assert!(model.results.is_none());
}

#[test]
fn numeric_editor_accepts_transient_values_but_not_malformed_paste() {
    for value in ["", "-", "+", ".", "-.", "-12.5", "1234567890123456"] {
        assert!(numeric_edit(value), "{value}");
    }
    for value in ["1.2.3", "1-2", "++", "NaN", "12\n3", "12345678901234567"] {
        assert!(!numeric_edit(value), "{value}");
    }
}
#[test]
fn rejects_unrelated_and_oversized_clipboard() {
    assert!(validate_item_text("personal clipboard".into()).is_err());
    assert!(validate_item_text(format!("Rarity: Rare\n--------\n{}", "x".repeat(65536))).is_err());
    assert!(
        validate_item_text(
            "Item Class: Helmets\nRarity: Unique\nGoldrim\nFelt Cap\n--------".into()
        )
        .is_ok()
    );
}

#[test]
fn stale_results_never_replace_new_item() {
    let mut m = EvaluateModel::default();
    let old = m.copy();
    let current = m.copy();
    m.failed(old, "old".into());
    m.searched(old, Results::default());
    assert!(m.copying);
    assert!(m.error.is_none());
    assert!(m.results.is_none());
    assert!(!m.copied(old, "old".into()));
    assert!(m.copied(current, "new".into()));
    m.dismiss();
    m.failed(current, "late".into());
    assert!(!m.visible);
}
#[test]
fn new_copy_cannot_retry_previous_items_text() {
    let mut model = EvaluateModel::default();
    let first = model.copy();
    assert!(model.copied(first, "old item".into()));
    let second = model.copy();
    model.failed(second, "copy failed".into());
    assert!(model.text.is_empty());
    assert!(model.evaluation.is_none());
}
#[test]
fn league_defaults_follow_metadata_without_overwriting_a_valid_choice() {
    let evaluation = Evaluation {
        leagues: vec![
            League {
                id: "Current".into(),
                text: "Current".into(),
            },
            League {
                id: "Hardcore".into(),
                text: "Hardcore".into(),
            },
        ],
        league: "Current".into(),
        ..Evaluation::default()
    };
    for saved in ["", "Expired"] {
        let mut model = EvaluateModel::new(saved.into());
        model.evaluated(0, evaluation.clone());
        assert_eq!(model.league, "Current");
    }
    let mut model = EvaluateModel::new("Hardcore".into());
    model.evaluated(0, evaluation);
    assert_eq!(model.league, "Hardcore");
}

fn observed_character(name: &str, league: Option<&str>) -> retinue_poe2::reader::Snapshot {
    use retinue_poe2::reader::{Identity, Player, Snapshot, Social};
    Snapshot {
        scene: "in_game".into(),
        player: Some(Player {
            name: name.into(),
            class: None,
            level: 10,
            experience: 0,
            gold: None,
            life: None,
        }),
        social: league.map(|league| Social {
            local: Some(Identity {
                character: name.into(),
                account: "Account".into(),
                league: league.into(),
            }),
            party: vec![],
        }),
        ..Snapshot::default()
    }
}

fn trade_leagues() -> Evaluation {
    Evaluation {
        leagues: ["Standard", "Current", "Hardcore"]
            .into_iter()
            .map(|id| League {
                id: id.into(),
                text: id.into(),
            })
            .collect(),
        league: "Current".into(),
        ..Evaluation::default()
    }
}

#[test]
fn native_league_applies_before_first_search_and_only_once_per_character() {
    let mut model = EvaluateModel::new("Standard".into());
    let session = observed_character("Exile", Some("Current"));
    assert!(!model.observe_session(&session));
    model.evaluated(0, trade_leagues());
    assert_eq!(model.league, "Current");
    model.league = "Hardcore".into();
    assert!(!model.observe_session(&session));
    assert!(!model.observe_session(&observed_character("Exile", None)));
    let copy = model.copy();
    model.copied(copy, "Item".into());
    model.evaluated(copy, trade_leagues());
    assert_eq!(model.league, "Hardcore");
    assert!(model.observe_session(&observed_character("Other", Some("Standard"))));
    assert_eq!(model.league, "Standard");
}

#[test]
fn late_identity_invalidates_old_prices_but_read_failures_do_not() {
    let mut model = EvaluateModel::new("Standard".into());
    model.evaluated(0, trade_leagues());
    let generation = model.generation;
    let prediction = model.prediction_generation;
    assert!(model.observe_session(&observed_character("Exile", Some("Current"))));
    assert_ne!(model.generation, generation);
    model.searched(generation, Results::default());
    model.predicted(
        prediction,
        Some(Estimate {
            amount: 2.0,
            currency: "exalted".into(),
            uncertainty: None,
        }),
    );
    assert!(model.results.is_none());
    assert!(model.estimate.is_none());
    let generation = model.generation;
    assert!(!model.observe_session(&retinue_poe2::reader::Snapshot::default()));
    assert!(!model.observe_session(&observed_character("Exile", None)));
    assert_eq!(model.generation, generation);
    assert_eq!(model.league, "Current");
}

#[test]
fn unavailable_leagues_do_not_guess_and_identity_waits_for_trade_metadata() {
    let mut model = EvaluateModel::new("Hardcore".into());
    model.evaluated(0, trade_leagues());
    assert!(!model.observe_session(&observed_character("Solo", Some("SSF Current"))));
    assert_eq!(model.league, "Hardcore");
    assert!(!model.observe_session(&observed_character("New", Some("Future"))));
    let mut evaluation = trade_leagues();
    evaluation.leagues.push(League {
        id: "Future".into(),
        text: "Future".into(),
    });
    model.evaluated(0, evaluation);
    assert_eq!(model.league, "Future");
}

#[test]
fn alt_tab_preserves_pending_results_and_restores_without_focus_theft() {
    let mut model = EvaluateModel::new("Standard".into());
    let id = model.copy();
    model.copied(id, "item".into());
    let mut presentation = Presentation {
        game_active: false,
        keyboard: true,
    };
    assert!(presentation.visible(&model));
    presentation.blur();
    assert!(!presentation.visible(&model));
    model.searched(
        id,
        Results {
            total: 42,
            ..Results::default()
        },
    );
    assert!(!presentation.visible(&model));
    presentation.game_active = true;
    assert!(presentation.visible(&model));
    assert!(!presentation.keyboard);
    assert_eq!(model.results.as_ref().unwrap().total, 42);
    model.dismiss();
    assert!(!presentation.visible(&model));
    presentation.game_active = false;
    presentation.game_active = true;
    assert!(!presentation.visible(&model));
}

#[test]
fn returning_to_game_does_not_reacquire_keyboard_after_layer_blur() {
    let model = EvaluateModel {
        visible: true,
        ..EvaluateModel::default()
    };
    let mut presentation = Presentation {
        game_active: true,
        keyboard: true,
    };
    presentation.blur();
    assert!(presentation.visible(&model));
    assert!(!presentation.keyboard);
    presentation.game_active = false;
    assert!(!presentation.visible(&model));
    presentation.game_active = true;
    assert!(presentation.visible(&model));
    assert!(!presentation.keyboard);
}
#[test]
fn filters_are_explicit_and_reject_invalid_ranges() {
    let mut m = EvaluateModel::new("Standard".into());
    m.evaluated(
        0,
        Evaluation {
            item: Item {
                modifiers: vec![Modifier {
                    id: Some("explicit.life".into()),
                    min: Some(80.0),
                    supported: true,
                    ..Modifier::default()
                }],
                ..Item::default()
            },
            leagues: vec![League {
                id: "Standard".into(),
                text: "Standard".into(),
            }],
            league: "Standard".into(),
            ..Evaluation::default()
        },
    );
    assert_eq!(m.search().unwrap()["filters"], json!([]));
    m.loading = false;
    m.filters[0] = Filter {
        enabled: true,
        min: "90".into(),
        max: "80".into(),
    };
    assert!(m.search().is_err());
    m.filters[0].max = "NaN".into();
    assert!(m.search().is_err());
}

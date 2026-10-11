%%%-------------------------------------------------------------------
%% Common Test for worldstate caching and search.
%%%-------------------------------------------------------------------
-module(wf_worldstate_SUITE).

-export([all/0,
         init_per_suite/1,
         end_per_suite/1,
         uses_cache_when_fresh/1,
         fetches_when_stale/1,
         fetch_timeout_uses_cache/1,
         search_matches/1,
         search_matches_fieldron/1,
         search_matches_faction_codes/1,
         search_matches_baro/1,
         search_matches_void_storms/1,
         search_matches_events/1,
         search_matches_calendar/1,
         search_raw_matches_calendar_identifier/1,
         search_matches_global_upgrades/1,
         search_matches_syndicate_missions/1,
         search_matches_daily_deals/1,
         search_matches_prime_vault/1,
         alerts_subcommand_lists_entries/1,
         alerts_table_output/1,
         prime_vault_inventory_table_columns/1,
         teshin_inventory_table_output/1,
         fissures_table_output/1,
         invasions_table_output/1,
         sorties_table_output/1,
         voidstorms_table_output/1,
         events_table_output/1,
         calendar_subcommand_lists_entries/1,
         calendar_table_output/1,
         calendar_day_filter/1,
         global_upgrades_table_output/1,
         syndicate_missions_table_output/1,
         daily_deals_table_output/1,
         prime_vault_table_output/1,
         baro_table_output/1,
         arbitration_table_output/1,
         archimedea_command_output/1,
         extra_subcommands_table_output/1,
         watch_once_runs_specs/1,
         watch_inline_default_colors_rows/1,
         watch_subcommand_diff_once/1,
         format_alias_table_output/1,
         alerts_time_local_default/1,
         alerts_time_raw/1,
         alert_block_includes_details/1,
         alert_block_includes_extra_fields/1,
         invasion_block_includes_progress/1,
         fissure_block_includes_hard/1,
         sortie_block_includes_variants/1,
         event_block_includes_link_flags/1,
         prime_vault_block_includes_featured/1,
         event_lang_selects_message/1,
         baro_inventory_entries_empty/1,
         prime_vault_inventory_entries/1,
         prime_vault_inventory_resolves_store_items/1,
         format_entry_text/1,
         local_time_formatting/1,
         raw_time_formatting/1,
         calendar_raw_keeps_identifiers/1,
         node_name_mapping/1,
         caches_item_map_on_first_use/1,
         faction_names_are_pretty/1,
         faction_codes_when_not_resolving/1,
         fissure_fields_are_pretty/1,
         resolves_items_when_requested/1,
         keeps_raw_ids_when_not_resolving/1]).
-include_lib("common_test/include/ct.hrl").
-include_lib("eunit/include/eunit.hrl").

all() ->
    [uses_cache_when_fresh,
     fetches_when_stale,
     fetch_timeout_uses_cache,
     search_matches,
     search_matches_fieldron,
     search_matches_faction_codes,
     search_matches_baro,
     search_matches_void_storms,
     search_matches_events,
     search_matches_calendar,
     search_raw_matches_calendar_identifier,
     search_matches_global_upgrades,
     search_matches_syndicate_missions,
     search_matches_daily_deals,
     search_matches_prime_vault,
     alerts_subcommand_lists_entries,
     alerts_table_output,
     prime_vault_inventory_table_columns,
     teshin_inventory_table_output,
     fissures_table_output,
     invasions_table_output,
     sorties_table_output,
     voidstorms_table_output,
     events_table_output,
     calendar_subcommand_lists_entries,
     calendar_table_output,
     calendar_day_filter,
     global_upgrades_table_output,
     syndicate_missions_table_output,
     daily_deals_table_output,
     prime_vault_table_output,
     baro_table_output,
     arbitration_table_output,
     archimedea_command_output,
     extra_subcommands_table_output,
     watch_once_runs_specs,
     watch_inline_default_colors_rows,
     watch_subcommand_diff_once,
     format_alias_table_output,
     alerts_time_local_default,
     alerts_time_raw,
     alert_block_includes_details,
     alert_block_includes_extra_fields,
     invasion_block_includes_progress,
     fissure_block_includes_hard,
     sortie_block_includes_variants,
     event_block_includes_link_flags,
     prime_vault_block_includes_featured,
     event_lang_selects_message,
     baro_inventory_entries_empty,
     prime_vault_inventory_entries,
     prime_vault_inventory_resolves_store_items,
     format_entry_text,
     local_time_formatting,
     raw_time_formatting,
     calendar_raw_keeps_identifiers,
     node_name_mapping,
     caches_item_map_on_first_use,
     faction_names_are_pretty,
     faction_codes_when_not_resolving,
     fissure_fields_are_pretty,
     resolves_items_when_requested,
     keeps_raw_ids_when_not_resolving].

init_per_suite(Config) ->
    {ok, _} = application:ensure_all_started(retinue_cli),
    ok = retinue_test_daemon:start(),
    Config.

end_per_suite(_Config) ->
    retinue_test_daemon:stop(),
    ok.

uses_cache_when_fresh(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    NowFun = fun() -> calendar:datetime_to_gregorian_seconds(calendar:universal_time()) end,
    FetchFun = fun() -> put(fetched_fresh, true), {ok, Bin} end,
    {ok, Ws, Source} = wf_worldstate:load(#{cache => Cache, ttl => 9999, now_fun => NowFun, fetch_fun => FetchFun}),
    ?assertEqual(cached, Source),
    ?assert(length(wf_worldstate:index(Ws)) > 0),
    ?assertEqual(undefined, get(fetched_fresh)).

fetches_when_stale(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Stale = fun() -> 0 end,
    Refetch = fun() -> put(fetched, true), {ok, Bin} end,
    {ok, _Ws, fetched} = wf_worldstate:load(#{cache => Cache, ttl => 1, now_fun => Stale, fetch_fun => Refetch}),
    ?assertEqual(true, get(fetched)).

fetch_timeout_uses_cache(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    FetchFail = fun() -> {error, timeout} end,
    {ok, Ws, cached_stale} = wf_worldstate:load(#{cache => Cache, ttl => 0, refresh => true, fetch_fun => FetchFail}),
    ?assert(length(wf_worldstate:index(Ws)) > 0).

search_matches(_Config) ->
    Ws = load_fixture_ws(),
    Results = wf_worldstate:search(Ws, "lith"),
    ?assert(lists:any(fun(E) -> maps:get(type, E) =:= fissure end, Results)).

search_matches_fieldron(_Config) ->
    Ws = load_fixture_ws(),
    Results = wf_worldstate:search(Ws, "Fieldron"),
    ?assert(length(Results) >= 1),
    [First | _] = Results,
    ?assertEqual(invasion, maps:get(type, First)).

search_matches_faction_codes(_Config) ->
    Ws = load_fixture_ws(#{search_raw => true}),
    Results = wf_worldstate:search(Ws, "FC_GRINEER"),
    ?assert(length(Results) >= 1),
    [First | _] = Results,
    ?assertEqual(invasion, maps:get(type, First)).

search_matches_baro(_Config) ->
    Ws = load_fixture_ws(),
    Results = wf_worldstate:search(Ws, "baro"),
    ?assert(lists:any(fun(E) -> maps:get(type, E) =:= baro end, Results)).

search_matches_void_storms(_Config) ->
    Ws = load_fixture_ws(),
    Results = wf_worldstate:search(Ws, "VoidT2"),
    ?assert(lists:any(fun(E) -> maps:get(type, E) =:= void_storm end, Results)).

search_matches_events(_Config) ->
    Ws = load_fixture_ws(),
    Results = wf_worldstate:search(Ws, "official warframe wiki"),
    ?assertEqual(1, length(Results)),
    [First | _] = Results,
    ?assertEqual(event, maps:get(type, First)).

search_matches_calendar(_Config) ->
    Ws = load_fixture_ws(),
    Results = wf_worldstate:search(Ws, "calendar"),
    ?assert(length(Results) >= 1),
    [First | _] = Results,
    ?assertEqual(calendar, maps:get(type, First)).

search_raw_matches_calendar_identifier(_Config) ->
    Ws = load_fixture_ws(#{resolve_items => false, search_raw => true}),
    Results = wf_worldstate:search(Ws, "Calendar1999"),
    ?assert(length(Results) >= 1),
    [First | _] = Results,
    ?assertEqual(calendar, maps:get(type, First)).

search_matches_global_upgrades(_Config) ->
    Ws = load_fixture_ws_with(#{<<"GlobalUpgrades">> => [sample_global_upgrade()]}),
    Results = wf_worldstate:search(Ws, "credit booster"),
    ?assertEqual(1, length(Results)),
    [First | _] = Results,
    ?assertEqual(global_upgrade, maps:get(type, First)).

search_matches_syndicate_missions(_Config) ->
    Ws = load_fixture_ws(),
    Results = wf_worldstate:search(Ws, "arbiters"),
    ?assertEqual(1, length(Results)),
    [First | _] = Results,
    ?assertEqual(syndicate_mission, maps:get(type, First)).

search_matches_daily_deals(_Config) ->
    Ws = load_fixture_ws_with(#{<<"DailyDeals">> => [sample_daily_deal()]}),
    Results = wf_worldstate:search(Ws, "detonite injector"),
    ?assert(lists:any(fun(E) -> maps:get(type, E) =:= daily_deal end, Results)).

search_matches_prime_vault(_Config) ->
    Ws = load_fixture_ws(),
    Results = wf_worldstate:search(Ws, "prime vault"),
    ?assert(length(Results) >= 1),
    ?assert(lists:any(fun(E) -> maps:get(type, E, undefined) =:= prime_vault end, Results)).

alerts_subcommand_lists_entries(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["alerts", "--cache", Cache, "--ttl", "999999999"])
    end),
    ?assert(string:find(Output, "Entries for alert") =/= nomatch).

alerts_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["alerts", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for alert") =/= nomatch).

prime_vault_inventory_table_columns(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["prime-vault", "--inventory", "--cache", Cache, "--ttl", "999999999",
                                  "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Name") =/= nomatch),
    ?assert(string:find(Output, "Price") =/= nomatch).

teshin_inventory_table_output(_Config) ->
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["teshin","--output-format", "table"])
    end),
    ?assert(string:find(Output, "Teshin data source: calculated") =/= nomatch),
    ?assert(string:find(Output, "Inventory entries: 20") =/= nomatch),
    ?assert(string:find(Output, "Steel Essence") =/= nomatch),
    ?assert(string:find(Output, "Evergreen") =/= nomatch),
    ?assertEqual(nomatch, string:find(Output, "Activation")),
    ?assertEqual(nomatch, string:find(Output, "Expiry")).

fissures_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["fissures", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for fissure") =/= nomatch).

invasions_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["invasions", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Node") =/= nomatch).

sorties_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["sorties", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for sortie") =/= nomatch).

voidstorms_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["voidstorms", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for void_storm") =/= nomatch).

events_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["events", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for event") =/= nomatch).

calendar_subcommand_lists_entries(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["calendar", "--cache", Cache, "--ttl", "999999999"])
    end),
    ?assert(string:find(Output, "Calendar:") =/= nomatch).

calendar_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["calendar", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Name") =/= nomatch),
    ?assert(string:find(Output, "Details") =/= nomatch).

calendar_day_filter(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["calendar", "--day", "194", "--cache", Cache, "--ttl", "999999999",
                                  "--output-format", "block"])
    end),
    ?assert(string:find(Output, "day 194") =/= nomatch),
    ?assert(string:find(Output, "day 191") =:= nomatch),
    ?assert(string:find(Output, "day 200") =:= nomatch).

global_upgrades_table_output(_Config) ->
    {Cache, Bin} = sample_cache_with(#{<<"GlobalUpgrades">> => [sample_global_upgrade()]}),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["global-upgrades", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Name") =/= nomatch),
    ?assert(string:find(Output, "Window") =/= nomatch).

syndicate_missions_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["syndicate-missions", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for syndicate_mission") =/= nomatch).

daily_deals_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["daily-deals", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for daily_deal") =/= nomatch).

prime_vault_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["prime-vault", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for prime_vault") =/= nomatch).

baro_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["baro", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for baro") =/= nomatch).

arbitration_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["arbitration", "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
    end),
    ?assert(string:find(Output, "Entries for arbitration") =/= nomatch).

archimedea_command_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["archimedea","--deep", "--cache", Cache, "--ttl", "999999999"])
    end),
    ?assert(string:find(Output, "Deep Archimedea") =/= nomatch),
    ?assert(string:find(Output, "Sealed Armor") =/= nomatch),
    ?assert(string:find(Output, "Commanding Culverins") =/= nomatch),
    ?assert(string:find(Output, "Unavailable:") =/= nomatch),
    ?assertEqual(nomatch, string:find(Output, "Temporal Archimedea")).

extra_subcommands_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Subcommands = [
        "flash-sales",
        "goals",
        "archimedea",
        "construction-projects",
        "descents",
        "endless-xp",
        "experiment-recommended",
        "featured-guilds",
        "hub-events",
        "in-game-market",
        "library",
        "lite-sorties",
        "node-overrides",
        "pvp-active-tournaments",
        "pvp-alternative-modes",
        "pvp-challenges",
        "persistent-enemies",
        "prime-access",
        "prime-token",
        "prime-vault-availabilities",
        "project-pct",
        "season-info",
        "sku-sales",
        "twitch-promos",
        "meta"
    ],
    lists:foreach(
      fun(Sub) ->
          Output = capture_output(fun() ->
              retinue_cli:main(warframe, [Sub, "--cache", Cache, "--ttl", "999999999", "--output-format", "table"])
          end),
          ?assert(string:find(Output, "Entries for") =/= nomatch)
      end,
      Subcommands
    ).

watch_once_runs_specs(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["watch", "--once", "--no-clear",
                                  "--cache", Cache, "--ttl", "999999999",
                                  "--spec", "invasions", "--spec", "fissures:lith"])
    end),
    ?assert(string:find(Output, "Worldstate watch") =/= nomatch),
    ?assert(string:find(Output, "== invasions ==") =/= nomatch),
    ?assert(string:find(Output, "fissures (lith)") =/= nomatch).

format_alias_table_output(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["fissures", "--cache", Cache, "--ttl", "999999999", "--format", "table"])
    end),
    ?assert(string:find(Output, "Mission") =/= nomatch).

alerts_time_local_default(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["fissures", "--cache", Cache, "--ttl", "999999999", "--output-format", "block"])
    end),
    ?assert(re:run(Output, "T[0-9]{2}:[0-9]{2}:[0-9]{2}Z", []) =:= nomatch),
    ?assert(re:run(Output, "[+-][0-9]{2}:[0-9]{2}", []) =/= nomatch).

alerts_time_raw(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["fissures", "--utc", "--raw", "--cache", Cache, "--ttl", "999999999", "--output-format", "block"])
    end),
    ?assert(re:run(Output, "T[0-9]{2}:[0-9]{2}:[0-9]{2}Z", []) =/= nomatch).

watch_inline_default_colors_rows(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["watch", "--once", "--no-clear",
                                  "--cache", Cache, "--ttl", "999999999",
                                  "--spec", "fissures"])
    end),
    ?assert(string:find(Output, "== fissures ==") =/= nomatch),
    ?assert(re:run(Output, "\\x1b\\[32m", []) =:= nomatch).

watch_subcommand_diff_once(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["fissures", "--watch", "--diff", "--once",
                                  "--no-clear",
                                  "--cache", Cache, "--ttl", "999999999"])
    end),
    ?assert(string:find(Output, "Worldstate watch") =/= nomatch),
    ?assert(string:find(Output, "Changes:") =/= nomatch),
    ?assert(string:find(Output, "added") =/= nomatch).

alert_block_includes_details(_Config) ->
    Entry = #{type => alert,
              id => <<"alert_test">>,
              name => <<"SolNode144">>,
              data => #{
                  <<"Expiry">> => #{<<"$date">> => #{<<"$numberLong">> => <<"1701430200000">>}},
                  <<"MissionInfo">> => #{
                      <<"missionType">> => <<"MT_SURVIVAL">>,
                      <<"location">> => <<"SolNode144">>,
                      <<"minEnemyLevel">> => 10,
                      <<"maxEnemyLevel">> => 20,
                      <<"faction">> => <<"FC_OROKIN">>,
                      <<"missionReward">> => #{<<"credits">> => 1000,
                                              <<"itemString">> => <<"Test Reward">>}
                  }
              }},
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Level") =/= nomatch),
    ?assert(string:find(Text, "Faction") =/= nomatch),
    ?assert(string:find(Text, "Reward") =/= nomatch).

alert_block_includes_extra_fields(_Config) ->
    Entry = #{type => alert,
              id => <<"alert_extra">>,
              name => <<"SolNode144">>,
              extra_fields => #{"Icon" => "/Lotus/Icon.png", "Tag" => "TestTag"},
              data => #{
                  <<"Expiry">> => #{<<"$date">> => #{<<"$numberLong">> => <<"1701430200000">>}},
                  <<"MissionInfo">> => #{
                      <<"missionType">> => <<"MT_SURVIVAL">>,
                      <<"location">> => <<"SolNode144">>,
                      <<"minEnemyLevel">> => 10,
                      <<"maxEnemyLevel">> => 20,
                      <<"faction">> => <<"FC_OROKIN">>,
                      <<"missionReward">> => #{<<"credits">> => 1000,
                                              <<"itemString">> => <<"Test Reward">>}
                  }
              }},
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Icon: /Lotus/Icon.png") =/= nomatch),
    ?assert(string:find(Text, "Tag: TestTag") =/= nomatch).

invasion_block_includes_progress(_Config) ->
    Entry = #{type => invasion,
              id => <<"inv_test">>,
              name => <<"SolNode144">>,
              data => #{
                  <<"AttackerFaction">> => <<"FC_GRINEER">>,
                  <<"DefenderFaction">> => <<"FC_CORPUS">>,
                  <<"Count">> => 50,
                  <<"Goal">> => 100
              }},
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Progress") =/= nomatch).

fissure_block_includes_hard(_Config) ->
    Entry = #{type => fissure,
              id => <<"fissure_test">>,
              name => <<"Fissure">>,
              data => #{
                  <<"Modifier">> => <<"VoidT1">>,
                  <<"MissionType">> => <<"MT_EXTERMINATION">>,
                  <<"Node">> => <<"SolNode144">>,
                  <<"Hard">> => true
              }},
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Hard") =/= nomatch).

sortie_block_includes_variants(_Config) ->
    Entry = #{type => sortie,
              id => <<"sortie_test">>,
              name => <<"Sortie">>,
              data => #{
                  <<"Boss">> => <<"SORTIE_BOSS_HYENA">>,
                  <<"Faction">> => <<"FC_CORPUS">>,
                  <<"Expiry">> => #{<<"$date">> => #{<<"$numberLong">> => <<"1701435600000">>}},
                  <<"Variants">> => [#{<<"missionType">> => <<"MT_RESCUE">>,
                                      <<"modifierType">> => <<"SORTIE_MODIFIER_SECONDARY_ONLY">>,
                                      <<"node">> => <<"SolNode210">>}]
              }},
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Stages") =/= nomatch),
    ?assert(string:find(Text, "Modifiers") =/= nomatch).

event_block_includes_link_flags(_Config) ->
    Entry = #{type => event,
              id => <<"event_test">>,
              name => <<"Event">>,
              data => #{
                  <<"Messages">> => [#{<<"LanguageCode">> => <<"en">>, <<"Message">> => <<"Test Event">>}],
                  <<"Prop">> => <<"https://example.test/event">>,
                  <<"Priority">> => true,
                  <<"Community">> => true,
                  <<"MobileOnly">> => false
              }},
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Link") =/= nomatch),
    ?assert(string:find(Text, "Flags") =/= nomatch).

prime_vault_block_includes_featured(_Config) ->
    Entry = #{type => prime_vault,
              id => <<"pv_test">>,
              name => <<"Prime Vault">>,
              data => #{
                  <<"Node">> => <<"TradeHUB1">>,
                  <<"ScheduleInfo">> => [#{<<"FeaturedItem">> => <<"Featured Item">>}]
              }},
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Featured") =/= nomatch).

event_lang_selects_message(_Config) ->
    Ws = load_fixture_ws(),
    [Entry | _] = wf_worldstate:search(Ws, "TennoConcert 2026"),
    Text = format_projected(Entry, #{resolve_items => true, event_lang => <<"fr">>}),
    ?assert(string:find(Text, "Regardez maintenant") =/= nomatch).

baro_inventory_entries_empty(_Config) ->
    Ws = load_fixture_ws(),
    Entries = wf_worldstate:inventory_entries(Ws, baro, #{resolve_items => false}),
    ?assertEqual(0, length(Entries)).

prime_vault_inventory_entries(_Config) ->
    Ws = load_fixture_ws(),
    Entries = wf_worldstate:inventory_entries(Ws, prime_vault, #{resolve_items => false}),
    ?assert(length(Entries) >= 3),
    [First | _] = Entries,
    ?assertEqual(prime_vault_item, maps:get(type, First)).

prime_vault_inventory_resolves_store_items(_Config) ->
    Ws = load_fixture_ws(),
    Entries = wf_worldstate:inventory_entries(Ws, prime_vault, #{resolve_items => true}),
    Names = [maps:get(name, E) || E <- Entries],
    ?assert(lists:any(fun(N) -> string:find(N, "Braton Prime") =/= nomatch end, Names)).

format_entry_text(_Config) ->
    Ws = load_fixture_ws(),
    [First | _] = [E || E <- wf_worldstate:search(Ws, "lith"),
                        maps:get(type, E) =:= fissure],
    Text = format_projected(First),
    ?assert(string:find(Text, "Fissure") =/= nomatch),
    ?assert(string:find(Text, "Expires") =/= nomatch).

local_time_formatting(_Config) ->
    Text = wf_worldstate_projector:expiry(1701430200000, #{raw => false}),
    ?assertEqual(match, re:run(Text, "[+-][0-9]{2}:[0-9]{2}$", [{capture, none}])),
    ?assertNotEqual($Z, lists:last(Text)).

raw_time_formatting(_Config) ->
    Text = wf_worldstate_projector:expiry(1701430200000, #{raw => true}),
    ?assertNotEqual($Z, lists:last(Text)),
    Utc = wf_worldstate_projector:expiry(1701430200000, #{raw => true, utc => true}),
    ?assertEqual($Z, lists:last(Utc)).

calendar_raw_keeps_identifiers(_Config) ->
    {Cache, Bin} = sample_cache(),
    ok = file:write_file(Cache, Bin),
    Output = capture_output(fun() ->
        retinue_cli:main(warframe, ["calendar", "--raw", "--cache", Cache, "--ttl", "999999999"])
    end),
    Flat = re:replace(Output, "\\s+", "", [global, {return, list}]),
    ?assert(string:find(Flat, "CalendarKillTechrotEnemiesWithAbilitiesEasy") =/= nomatch).

node_name_mapping(_Config) ->
    Base = fissure_entry(<<"VoidT1">>, <<"MT_EXTERMINATION">>),
    Entry = Base#{data := maps:put(<<"Node">>, <<"SolNode43">>, maps:get(data, Base))},
    ?assert(string:find(format_projected(Entry, #{resolve_items => true}), "Cerberus") =/= nomatch).

faction_names_are_pretty(_Config) ->
    Entry = invasion_entry(<<"DummyItem">>),
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Corpus") =/= nomatch),
    ?assert(string:find(Text, "FC_") =:= nomatch).

faction_codes_when_not_resolving(_Config) ->
    Entry = invasion_entry(<<"DummyItem">>),
    Text = format_projected(Entry, #{resolve_items => false}),
    ?assert(string:find(Text, "FC_CORPUS") =/= nomatch),
    ?assert(string:find(Text, "Corpus") =:= nomatch).

fissure_fields_are_pretty(_Config) ->
    Entry = fissure_entry(<<"VoidT1">>, <<"MT_EXTERMINATION">>),
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Lith") =/= nomatch),
    ?assert(string:find(Text, "Exterminate") =/= nomatch).

caches_item_map_on_first_use(_Config) ->
    Prev = persistent_term:get({wfcli, item_map}, undefined),
    persistent_term:erase({wfcli, item_map}),
    _ = format_projected(invasion_entry(<<"DummyItemPath">>), #{resolve_items => true}),
    Map = persistent_term:get({wfcli, item_map}, undefined),
    ?assert(is_map(Map)),
    restore_item_map(Prev).

resolves_items_when_requested(_Config) ->
    %% Seed a minimal item map and ensure resolution occurs when enabled.
    ItemPath = <<"/Lotus/Types/Recipes/Weapons/WeaponParts/DeraVandalBarrel">>,
    persistent_term:put({wfcli, item_map}, #{ItemPath => <<"Dera Vandal Barrel">>}),
    Entry = invasion_entry(ItemPath),
    Text = format_projected(Entry, #{resolve_items => true}),
    ?assert(string:find(Text, "Dera Vandal Barrel") =/= nomatch),
    persistent_term:erase({wfcli, item_map}).

keeps_raw_ids_when_not_resolving(_Config) ->
    ItemPath = <<"/Lotus/Types/Recipes/Weapons/WeaponParts/DeraVandalBarrel">>,
    persistent_term:erase({wfcli, item_map}),
    Entry = invasion_entry(ItemPath),
    Text = format_projected(Entry, #{resolve_items => false}),
    ?assert(string:find(Text, "Dera Vandal Barrel") =:= nomatch),
    ?assert(string:find(Text, "/Lotus/Types/Recipes/Weapons/WeaponParts/DeraVandalBarrel") =/= nomatch).

format_projected(Entry) ->
    format_projected(Entry, #{}).

format_projected(Entry, Opts) ->
    Projected0 = wf_entity_worldstate:build(
                   maps:get(type, Entry),
                   maps:get(id, Entry, undefined),
                   maps:get(name, Entry, undefined),
                   maps:get(data, Entry, #{}),
                   Opts),
    ExplicitExtras = maps:get(extra_fields, Entry, #{}),
    Projected = Projected0#{
                  extra_fields => maps:merge(maps:get(extra_fields, Projected0, #{}),
                                             ExplicitExtras)},
    wf_worldstate_format:format(Projected, Opts).

load_fixture_ws() ->
    load_fixture_ws(#{}).

load_fixture_ws(Opts) ->
    {Cache, Bin} = sample_cache(),
    load_fixture_bin(Cache, Bin, Opts).

load_fixture_ws_with(Overrides) ->
    {Cache, Bin} = sample_cache_with(Overrides),
    load_fixture_bin(Cache, Bin, #{}).

load_fixture_bin(Cache, Bin, Opts) ->
    LoadOpts = maps:merge(#{cache => Cache, refresh => true, fetch_fun => fun() -> {ok, Bin} end}, Opts),
    {ok, Ws, _} = wf_worldstate:load(LoadOpts),
    Ws.

sample_cache() ->
    File = filename:join([code:lib_dir(retinue_cli), "test", "fixtures", "worldstate_sample.json"]),
    {ok, Bin} = file:read_file(File),
    BaseTmp = case os:getenv("TMPDIR") of false -> "/tmp"; undefined -> "/tmp"; V -> V end,
    Tmp = filename:join([BaseTmp, "wf_worldstate_cache.json"]),
    {Tmp, Bin}.

sample_cache_with(Overrides) ->
    {_Cache, Bin} = sample_cache(),
    Raw = jsone:decode(Bin, [{object_format, map}]),
    BaseTmp = case os:getenv("TMPDIR") of false -> "/tmp"; undefined -> "/tmp"; V -> V end,
    Cache = filename:join([BaseTmp, "wf_worldstate_cache_overrides.json"]),
    {Cache, jsone:encode(maps:merge(Raw, Overrides))}.

sample_global_upgrade() ->
    #{<<"_id">> => #{<<"$oid">> => <<"global-upgrade-test">>},
      <<"Activation">> => #{<<"$date">> => #{<<"$numberLong">> => <<"1783807200000">>}},
      <<"Expiry">> => #{<<"$date">> => #{<<"$numberLong">> => <<"1784412000000">>}},
      <<"LocTag">> => <<"Credit Booster">>,
      <<"Upgrade">> => <<"Credit Booster">>}.

sample_daily_deal() ->
    #{<<"Activation">> => #{<<"$date">> => #{<<"$numberLong">> => <<"1701420000000">>}},
      <<"Expiry">> => #{<<"$date">> => #{<<"$numberLong">> => <<"4102444800000">>}},
      <<"AmountSold">> => 10,
      <<"AmountTotal">> => 100,
      <<"Discount">> => 50,
      <<"OriginalPrice">> => 100,
      <<"SalePrice">> => 50,
      <<"StoreItem">> => <<"/Lotus/StoreItems/Types/Items/Research/ChemComponent">>}.

invasion_entry(ItemPath) ->
    #{type => invasion,
      id => <<"inv_test">>,
      name => <<"Test">>,
      data => #{
        <<"AttackerFaction">> => <<"FC_CORPUS">>,
        <<"DefenderFaction">> => <<"FC_GRINEER">>,
        <<"AttackerReward">> => #{
          <<"countedItems">> => [
            #{<<"ItemType">> => ItemPath, <<"ItemCount">> => 1}
          ]
        },
        <<"DefenderReward">> => #{}
      }}.

fissure_entry(Tier, MissionType) ->
    #{type => fissure,
      id => <<"fissure_test">>,
      name => <<"Fissure">>,
      data => #{
        <<"Modifier">> => Tier,
        <<"MissionType">> => MissionType,
        <<"Node">> => <<"SolNode144">>
      }}.

restore_item_map(undefined) ->
    persistent_term:erase({wfcli, item_map});
restore_item_map(Prev) ->
    persistent_term:put({wfcli, item_map}, Prev).

capture_output(Fun) ->
    Capturer = spawn(fun() -> io_capture_loop([]) end),
    Old = group_leader(),
    group_leader(Capturer, self()),
    try
        _ = Fun()
    after
        group_leader(Old, self())
    end,
    Capturer ! {get, self()},
    receive
        {captured, Output} -> to_list(Output)
    after 1000 ->
        ""
    end.

io_capture_loop(Acc) ->
    receive
        {io_request, From, ReplyAs, Request} ->
            {NewAcc, Reply} = handle_io_request(Request, Acc),
            From ! {io_reply, ReplyAs, Reply},
            io_capture_loop(NewAcc);
        {get, Requestor} ->
            Requestor ! {captured, lists:flatten(lists:reverse(Acc))},
            io_capture_loop(Acc)
    end.

handle_io_request({put_chars, Chars}, Acc) ->
    {[Chars | Acc], ok};
handle_io_request({put_chars, _Enc, Chars}, Acc) ->
    {[Chars | Acc], ok};
handle_io_request({put_chars, Enc, Mod, Fun, Args}, Acc) ->
    handle_io_request({put_chars, Enc, apply(Mod, Fun, Args)}, Acc);
handle_io_request({put_chars, Mod, Fun, Args}, Acc) ->
    handle_io_request({put_chars, apply(Mod, Fun, Args)}, Acc);
handle_io_request({format, Format, Args}, Acc) ->
    {[io_lib:format(Format, Args) | Acc], ok};
handle_io_request({fwrite, Format, Args}, Acc) ->
    {[io_lib:format(Format, Args) | Acc], ok};
handle_io_request({requests, Reqs}, Acc) when is_list(Reqs) ->
    lists:foldl(
      fun(Req, {Acc0, _Reply0}) -> handle_io_request(Req, Acc0) end,
      {Acc, ok},
      Reqs);
handle_io_request(_Req, Acc) ->
    {Acc, ok}.

to_list(V) when is_binary(V) -> binary_to_list(V);
to_list(V) when is_atom(V) -> atom_to_list(V);
to_list(V) when is_list(V) -> V;
to_list(V) -> io_lib:format("~p", [V]).

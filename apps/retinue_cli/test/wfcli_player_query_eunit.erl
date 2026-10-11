%%%-------------------------------------------------------------------
%% EUnit coverage for player dataset query projection.
%%%-------------------------------------------------------------------
-module(wfcli_player_query_eunit).

-include_lib("eunit/include/eunit.hrl").

inventory_provenance_does_not_duplicate_auto_search_test() ->
    Full = wfcli_player_inventory_eunit:full(1, 12),
    {ok, Data} = wfcli_player_inventory:apply(<<"inventory_http">>, Full, #{}, wfcli_player_inventory:boot_id()),
    Snapshot = #{revision => 1234, updated_at => 1234, data => Data},
    {ok, Ast} = wfcli_query_parse:parse("resource"),
    {ok, #{results := Results}} = wfcli_player_query:execute(Ast, #{}, Snapshot),
    ?assertNot(lists:any(fun(Entry) -> maps:get(source, Entry) =:= <<"inventory_http">> end,
                         maps:get(slice, Results))),
    {ok, RawQuery} = wfcli_query_parse:parse("source=inventory_http data.raw.MiscItems.0.ItemCount=12"),
    {ok, #{results := RawResults}} = wfcli_player_query:execute(RawQuery, #{}, Snapshot),
    ?assertEqual(1, maps:get(total, RawResults)).

source_filter_uses_shared_query_engine_test() ->
    {ok, Ast} = wfcli_query_parse:parse("source=game data.phase=launcher"),
    {ok, #{results := Results}} = wfcli_player_query:execute(Ast, #{}, snapshot()),
    [Entry] = maps:get(slice, Results),
    ?assertEqual(<<"game">>, maps:get(source, Entry)).

raw_root_supports_nested_data_path_test() ->
    {ok, Ast} = wfcli_query_parse:parse("data.inventory.weapons.0=Braton"),
    {ok, #{results := Results}} = wfcli_player_query:execute(Ast, #{}, snapshot()),
    [Entry] = maps:get(slice, Results),
    ?assertEqual(player, maps:get(type, Entry)).

boolean_terms_search_raw_player_data_test() ->
    {ok, Ast} = wfcli_query_parse:parse("braton OR launcher"),
    {ok, #{results := Results}} = wfcli_player_query:execute(Ast, #{}, snapshot()),
    ?assert(maps:get(total, Results) >= 2).

typed_inventory_entities_use_normal_query_fields_test() ->
    {ok, Ast} = wfcli_query_parse:parse("type=player_stack count>=2"),
    {ok, #{results := Results}} =
        wfcli_player_query:execute(Ast, #{}, typed_snapshot()),
    [Entry] = maps:get(slice, Results),
    ?assertEqual(<<"/Lotus/Types/Items/MiscItems/ArgonCrystal">>,
                 maps:get(item_type, Entry)),
    ?assertEqual(3, maps:get(count, Entry)).

typed_mastery_and_recipe_entities_are_distinct_test() ->
    {ok, Ast} = wfcli_query_parse:parse(
                  "type=player_mastery OR type=player_recipe"),
    {ok, #{results := Results}} =
        wfcli_player_query:execute(Ast, #{}, typed_snapshot()),
    ?assertEqual([player_mastery, player_recipe],
                 lists:sort([maps:get(type, Entry)
                             || Entry <- maps:get(slice, Results)])).

auto_view_prefers_typed_record_for_same_origin_test() ->
    {ok, Ast} = wfcli_query_parse:parse(
                  "origin=inventory.raw.Suits.0"),
    {ok, #{results := Results}} =
        wfcli_player_query:execute(Ast, #{}, fixture_snapshot()),
    [Entry] = maps:get(slice, Results),
    ?assertEqual(player_equipment, maps:get(type, Entry)),
    ?assertEqual(<<"typed">>, maps:get(representation, Entry)),
    ?assertEqual(<<"inventory.raw.Suits.0">>, maps:get(origin, Entry)).

view_control_can_select_raw_typed_or_both_test() ->
    Base = "origin=inventory.raw.Suits.0",
    ?assertEqual([player_raw], query_types("view=raw " ++ Base)),
    ?assertEqual([player_equipment], query_types("view=typed " ++ Base)),
    ?assertEqual([player_equipment, player_raw],
                 lists:sort(query_types("view=both " ++ Base))).

typed_view_contains_only_typed_entries_test() ->
    {ok, Ast} = wfcli_query_parse:parse("view=typed"),
    {ok, #{results := Results}} =
        wfcli_player_query:execute(Ast, #{}, fixture_snapshot()),
    Entries = maps:get(slice, Results),
    ?assert(Entries =/= []),
    ?assert(lists:all(
              fun(Entry) -> maps:get(representation, Entry) =:= <<"typed">> end,
              Entries)).

nested_projection_origin_supports_raw_typed_or_both_test() ->
    Base = "origin=inventory.raw.Suits.0.Configs.0",
    ?assertEqual([player_config], query_types(Base)),
    ?assertEqual([player_raw], query_types("view=raw " ++ Base)),
    ?assertEqual([player_config, player_raw],
                 lists:sort(query_types("view=both " ++ Base))).

typed_path_queries_enriched_nested_fields_test() ->
    {ok, Ast} = wfcli_query_parse:parse(
                  "type=player_equipment typed.configs.0.upgrade_slots.0.rank=8"),
    {ok, #{results := Results}} =
        wfcli_player_query:execute(Ast, #{}, fixture_snapshot()),
    [Entry] = maps:get(slice, Results),
    ?assertEqual(<<"suit-1">>, maps:get(instance_id, Entry)).

typed_scalar_fields_are_directly_queryable_test() ->
    ?assertEqual([player_profile], query_types("player_level=18")),
    ?assertEqual([player_loadout], query_types("type=player_loadout active=true")),
    ?assertEqual([player_upgrade],
                 query_types("type=player_upgrade equipped=true rank=8")).

secondary_origin_resolves_to_covering_typed_record_test() ->
    ?assertEqual([player_profile],
                 query_types("origin=inventory.raw.PlayerLevel")),
    ?assertEqual([player_raw],
                 query_types("view=raw origin=inventory.raw.PlayerLevel")).

typed_record_keeps_unprojected_source_fields_queryable_test() ->
    Snapshot0 = fixture_snapshot(),
    Inventory0 = maps:get(<<"inventory">>, maps:get(data, Snapshot0)),
    Raw0 = maps:get(<<"raw">>, Inventory0),
    [Suit0] = maps:get(<<"Suits">>, Raw0),
    Raw = Raw0#{<<"Suits">> => [Suit0#{<<"FutureStat">> => 17}]},
    Snapshot = Snapshot0#{data => #{<<"inventory">> =>
                                        Inventory0#{<<"raw">> => Raw}}},
    {ok, Ast} = wfcli_query_parse:parse(
                  "type=player_equipment data.FutureStat=17"),
    {ok, #{results := Results}} = wfcli_player_query:execute(Ast, #{}, Snapshot),
    [Entry] = maps:get(slice, Results),
    ?assertEqual(<<"suit-1">>, maps:get(instance_id, Entry)).

unprojected_raw_data_remains_in_auto_view_test() ->
    {ok, Ast} = wfcli_query_parse:parse(
                  "data.UnknownFutureSection.kept=true"),
    {ok, #{results := Results}} =
        wfcli_player_query:execute(Ast, #{}, fixture_snapshot()),
    [Entry] = maps:get(slice, Results),
    ?assertEqual(player_raw, maps:get(type, Entry)),
    ?assertEqual(<<"raw">>, maps:get(representation, Entry)).

invalid_view_control_is_rejected_test() ->
    {ok, Ast} = wfcli_query_parse:parse("view=merged type=player_equipment"),
    ?assertMatch({error, {query_errors, [_]}},
                 wfcli_player_query:execute(Ast, #{}, fixture_snapshot())).

snapshot() ->
    #{revision => 4, updated_at => 1234,
      data => #{
          <<"game">> => #{<<"phase">> => <<"launcher">>, <<"running">> => false},
          <<"inventory">> => #{<<"weapons">> => [<<"Braton">>, <<"Paris">>]}
      }}.

typed_snapshot() ->
    #{revision => 8, updated_at => 5678,
      data => #{
          <<"inventory">> => #{
              <<"schema">> => 1,
              <<"profile">> => #{<<"player_level">> => 18},
              <<"index">> => #{
                  <<"equipment">> => [
                      #{<<"collection">> => <<"Suits">>,
                        <<"item_type">> => <<"/Lotus/Powersuits/Excalibur">>,
                        <<"instance_id">> => <<"suit-1">>,
                        <<"count">> => 1, <<"xp">> => 9000}
                  ],
                  <<"stacks">> => [
                      #{<<"collection">> => <<"MiscItems">>,
                        <<"item_type">> =>
                            <<"/Lotus/Types/Items/MiscItems/ArgonCrystal">>,
                        <<"count">> => 3}
                  ],
                  <<"mastery">> => [
                      #{<<"item_type">> => <<"/Lotus/Weapons/Tenno/Rifle/Braton">>,
                        <<"xp">> => 450000}
                  ],
                  <<"pending_recipes">> => [
                      #{<<"item_type">> => <<"/Lotus/Weapons/Tenno/Rifle/Braton">>,
                        <<"instance_id">> => <<"recipe-1">>,
                        <<"completion_date">> => null}
                  ],
                  <<"missions">> => [],
                  <<"player_skills">> => #{<<"LPS_PILOTING">> => 7}
              },
              <<"raw">> => #{<<"UnknownFutureField">> => true}
          }
      }}.

query_types(Query) ->
    {ok, Ast} = wfcli_query_parse:parse(Query),
    {ok, #{results := Results}} =
        wfcli_player_query:execute(Ast, #{}, fixture_snapshot()),
    [maps:get(type, Entry) || Entry <- maps:get(slice, Results)].

fixture_snapshot() ->
    {ok, Body} = file:read_file(
                   "apps/retinue_cli/test/fixtures/player_inventory_sample.json"),
    Raw = jsone:decode(Body, [{object_format, map}]),
    #{revision => 9, updated_at => 6789,
      data => #{<<"inventory">> => #{
          <<"schema">> => 2,
          <<"profile">> => #{<<"player_name">> => <<"TestTenno">>},
          <<"raw">> => Raw}}}.

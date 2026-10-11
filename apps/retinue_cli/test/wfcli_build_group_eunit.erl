%%%-------------------------------------------------------------------
%% EUnit coverage for persisted build-group semantics.
%%%-------------------------------------------------------------------
-module(wfcli_build_group_eunit).

-include_lib("eunit/include/eunit.hrl").

rearrangement_is_default_and_explicit_choice_persists_test() ->
    {ok, Group} = wfcli_build_group:create(#{<<"definition_id">> => <<"/item">>},
                                          equipment(), 100, <<"group-1">>),
    ?assertEqual(false, maps:get(<<"preserve_source_slots">>, maps:get(<<"options">>, Group))),
    {ok, Locked} = wfcli_build_group:update(
                     Group, #{<<"options">> => #{<<"preserve_source_slots">> => true}},
                     equipment(), 200),
    {ok, Renamed} = wfcli_build_group:update(Locked, #{<<"name">> => <<"Renamed">>},
                                            equipment(), 300),
    ?assertEqual(true, maps:get(<<"preserve_source_slots">>, maps:get(<<"options">>, Renamed))).

captures_config_without_copying_instance_state_into_config_test() ->
    {ok, Group0} = wfcli_build_group:create(
                     #{<<"definition_id">> => <<"/item">>,
                       <<"instance_id">> => <<"copy-1">>},
                     equipment(), 100, <<"group-1">>),
    Baseline = maps:get(<<"baseline">>, Group0),
    ?assertEqual([#{<<"slot_id">> => <<"shard-1">>, <<"upgrade">> => <<"red">>}],
                 maps:get(<<"shard_slots">>, Baseline)),
    {ok, Group1} = wfcli_build_group:add_config(
                     Group0, <<"copy-1">>, 1, equipment(), 200),
    [Member] = maps:get(<<"members">>, Group1),
    ?assertEqual(<<"player_config">>, maps:get(<<"kind">>, Member)),
    Snapshot = maps:get(<<"snapshot">>, Member),
    Config = maps:get(<<"config">>, Snapshot),
    ?assertEqual([<<"/ability/roar">>], maps:get(<<"ability_override">>, Config)),
    ?assertNot(maps:is_key(<<"shard_slots">>, Snapshot)),
    ?assertNot(maps:is_key(<<"effective_polarities">>, Snapshot)),
    ?assertNot(maps:is_key(<<"shard_slots">>, Config)),
    ?assertEqual(2, maps:get(<<"revision">>, Group1)).

removing_current_config_keeps_owned_instance_baseline_test() ->
    {ok, Group0} = wfcli_build_group:create(
                     #{<<"definition_id">> => <<"/item">>,
                       <<"instance_id">> => <<"copy-1">>},
                     equipment(), 100, <<"group-1">>),
    Baseline = maps:get(<<"baseline">>, Group0),
    {ok, Group1} = wfcli_build_group:add_config(
                     Group0, <<"copy-1">>, 0, equipment(), 200),
    [Member] = maps:get(<<"members">>, Group1),
    {ok, Group2} = wfcli_build_group:remove_member(
                     Group1, maps:get(<<"id">>, Member), 300),
    ?assertEqual([], maps:get(<<"members">>, Group2)),
    ?assertEqual(<<"copy-1">>, maps:get(<<"instance_id">>, Group2)),
    ?assertEqual(Baseline, maps:get(<<"baseline">>, Group2)).

accepts_multiple_compatible_members_and_rejects_other_items_test() ->
    {ok, Group0} = wfcli_build_group:create(
                     #{<<"definition_id">> => <<"/item">>},
                     equipment(), 100, <<"group-1">>),
    {ok, Group1} = wfcli_build_group:add_source(Group0, revision(1, <<"/item">>),
                                                200),
    {ok, Group2} = wfcli_build_group:add_source(Group1, revision(2, <<"/item">>),
                                                300),
    ?assertEqual(2, length(maps:get(<<"members">>, Group2))),
    ?assertMatch({error, build_group_item_mismatch},
                 wfcli_build_group:add_source(Group2, revision(3, <<"/other">>),
                                              400)),
    Public = wfcli_build_group:public(Group2),
    ?assertEqual(2, maps:get(<<"member_count">>, Public)),
    ?assertEqual(2, maps:get(<<"source_member_count">>, Public)).

equipment() ->
    #{<<"definitions">> => [#{<<"id">> => <<"/item">>,
                               <<"name">> => <<"Test Item">>}],
      <<"instances">> =>
          [#{<<"instance_id">> => <<"copy-1">>,
             <<"definition_id">> => <<"/item">>,
             <<"class">> => <<"warframe">>,
             <<"capacity">> => 30,
             <<"forma_count">> => 2,
             <<"topology">> => #{<<"schema">> => 1, <<"regions">> => []},
             <<"effective_polarities">> =>
                 [#{<<"slot_id">> => <<"mod-1">>,
                    <<"polarity">> => <<"madurai">>}],
             <<"shard_slots">> =>
                 [#{<<"slot_id">> => <<"shard-1">>,
                    <<"upgrade">> => <<"red">>}],
             <<"configs">> =>
                 [#{<<"config_index">> => 0, <<"name">> => <<"A">>,
                    <<"upgrade_slots">> => []},
                  #{<<"config_index">> => 1, <<"name">> => <<"Roar">>,
                    <<"ability_override">> => [<<"/ability/roar">>],
                    <<"upgrade_slots">> =>
                        [#{<<"slot">> => 0, <<"topology_slot">> => <<"mod-1">>}]}]}]}.

revision(Id, Item) ->
    #{<<"identity">> => #{<<"source">> => <<"overframe">>,
                            <<"external_id">> => Id},
      <<"fingerprint">> => <<"fingerprint-", (integer_to_binary(Id))/binary>>,
      <<"content">> => #{<<"item">> => Item, <<"slots">> => []},
      <<"metadata">> => #{<<"title">> => <<"Build ",
                                              (integer_to_binary(Id))/binary>>}}.

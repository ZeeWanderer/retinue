%%%-------------------------------------------------------------------
%% EUnit coverage for build-group to Forma conversion.
%%%-------------------------------------------------------------------
-module(wfcli_build_plan_eunit).

-include_lib("eunit/include/eunit.hrl").

group_conversion_keeps_config_and_instance_state_separate_test() ->
    Group = group(),
    {ok, Request} = wfcli_build_plan:request(Group),
    [Raw] = maps:get(config_data, Request),
    ?assertNot(maps:is_key(<<"shard_slots">>, maps:get(item, Raw))),
    [Player, Source] = maps:get(builds, Raw),
    ?assertEqual([<<"/ability/roar">>],
                 maps:get(<<"ability_override">>, Player)),
    ?assertEqual([#{<<"name">> => <<"Arcane Energize">>, <<"rank">> => 5}],
                 maps:get(<<"arcanes">>, Player)),
    [SourceMod] = maps:get(<<"mods">>, Source),
    ?assertEqual(<<"vazarin">>, maps:get(<<"polarity">>, SourceMod)),
    ?assertEqual(8, maps:get(<<"cost">>, SourceMod)),
    {ok, FormaReply} = wfcli_forma_service:plan_request(Request),
    {ok, Result} = wfcli_build_plan:result(Group, {ok, FormaReply}),
    ?assertEqual(<<"ready">>, maps:get(<<"status">>, Result)),
    ?assertEqual(maps:get(<<"shard_slots">>, maps:get(<<"baseline">>, Group)),
                 maps:get(<<"shard_slots">>, Result)),
    [PlayerResult, _SourceResult] = maps:get(<<"builds">>, Result),
    ?assertEqual([<<"/ability/roar">>],
                 maps:get(<<"ability_override">>, PlayerResult)).

source_only_plan_still_uses_owned_instance_polarities_test() ->
    Group = (group())#{<<"members">> => [source_member()]},
    {ok, Request} = wfcli_build_plan:request(Group),
    [Raw] = maps:get(config_data, Request),
    ?assertEqual([<<"madurai">>, <<"none">>],
                 maps:get(<<"slots">>, maps:get(item, Raw))),
    [Source] = maps:get(builds, Raw),
    ?assertEqual(<<"source:1">>, maps:get(<<"member_id">>, Source)).

source_plan_requires_daemon_presentation_test() ->
    Source = source_member(),
    Snapshot = maps:remove(<<"presentation">>, maps:get(<<"snapshot">>, Source)),
    Group = (group())#{<<"members">> => [Source#{<<"snapshot">> => Snapshot}]},
    ?assertEqual(
       {error, {invalid_build_group_member, <<"source:1">>,
                build_source_presentation_required}},
       wfcli_build_plan:request(Group)).

planning_requires_concrete_instance_and_members_test() ->
    ?assertEqual({error, build_group_instance_required},
                 wfcli_build_plan:request((group())#{<<"baseline">> => null})),
    ?assertEqual({error, build_group_members_required},
                 wfcli_build_plan:request((group())#{<<"members">> => []})).

result_contains_every_renderable_loadout_test() ->
    Group = group(),
    {ok, Request} = wfcli_build_plan:request(Group),
    {ok, #{results := [{ok, Config, _, _}]}} = wfcli_forma_service:plan_request(Request),
    Plan = #{aura => madurai, exilus => none, 1 => vazarin, 2 => vazarin},
    Cost = wfcli_forma_rules:cost(Plan, maps:get(item, Config), #{}),
    {ok, Result} = wfcli_build_plan:result(Group, {ok, #{results => [{ok, Config, Plan, Cost}]}}),
    ?assertEqual(2, maps:get(<<"forma_count">>, Result)),
    ?assertEqual(#{<<"standard">> => 2}, maps:get(<<"forma_requirements">>, Result)),
    [Player, Source] = maps:get(<<"builds">>, Result),
    ?assertEqual(<<"config:1">>, maps:get(<<"member_id">>, Player)),
    ?assertEqual(<<"source:1">>, maps:get(<<"member_id">>, Source)),
    [Mod, Arcane] = maps:get(<<"upgrade_slots">>, Player),
    ?assertMatch(#{<<"topology_slot">> := <<"mod-1">>, <<"effective_drain">> := 4,
                   <<"polarity_state">> := <<"matched">>, <<"rank">> := 6}, Mod),
    ?assertMatch(#{<<"topology_slot">> := <<"arcane-1">>, <<"rank">> := 5}, Arcane),
    lists:foreach(fun(Build) ->
        ?assertEqual(60, maps:get(<<"capacity">>, Build)),
        ?assertEqual(4, maps:get(<<"drain">>, Build)),
        ?assertEqual(56, maps:get(<<"remaining_capacity">>, Build))
    end, [Player, Source]).

free_assignment_preserves_duplicate_mod_identity_test() ->
    Source = source_member(),
    Snapshot = maps:get(<<"snapshot">>, Source),
    [Mod] = maps:get(<<"upgrades">>, maps:get(<<"presentation">>, Snapshot)),
    Upgrades = [Mod#{<<"asset">> => #{<<"id">> => <<"first">>}},
                Mod#{<<"topology_slot">> => <<"mod-1">>,
                     <<"cost">> => 12, <<"asset">> => #{<<"id">> => <<"second">>}}],
    Member = Source#{<<"snapshot">> => Snapshot#{<<"presentation">> => #{<<"upgrades">> => Upgrades}}},
    Group = (group())#{<<"members">> => [Member],
                       <<"options">> => #{<<"preserve_source_slots">> => false}},
    {ok, Request} = wfcli_build_plan:request(Group),
    {ok, Reply} = wfcli_forma_service:plan_request(Request),
    {ok, Result} = wfcli_build_plan:result(Group, {ok, Reply}),
    [Build] = maps:get(<<"builds">>, Result),
    [First, Second] = maps:get(<<"upgrade_slots">>, Build),
    ?assertEqual(8, maps:get(<<"drain">>, First)),
    ?assertEqual(12, maps:get(<<"drain">>, Second)),
    ?assertEqual(#{<<"id">> => <<"first">>}, maps:get(<<"asset">>, First)),
    ?assertNotEqual(maps:get(<<"topology_slot">>, First), maps:get(<<"topology_slot">>, Second)).

weapon_reordering_requires_known_elements_test() ->
    Source = source_member(),
    Group = (group())#{<<"members">> => [Source],
                      <<"baseline">> => (baseline())#{<<"class">> => <<"primary">>},
                      <<"options">> => #{<<"preserve_source_slots">> => false}},
    ?assertMatch({error, {invalid_build_group_member, _, {unknown_mod_elements, _}}},
                 wfcli_build_plan:request(Group)),
    Snapshot = maps:get(<<"snapshot">>, Source),
    [Mod] = maps:get(<<"upgrades">>, maps:get(<<"presentation">>, Snapshot)),
    Member = Source#{<<"snapshot">> => Snapshot#{<<"presentation">> =>
               #{<<"upgrades">> => [Mod#{<<"elemental_types">> => [<<"cold">>]}]}}},
    {ok, Request} = wfcli_build_plan:request(Group#{<<"members">> => [Member]}),
    [Raw] = maps:get(config_data, Request),
    [Build] = maps:get(builds, Raw),
    ?assertEqual([<<"cold">>], maps:get(<<"elemental_order">>, Build)),
    [Converted] = maps:get(<<"mods">>, Build),
    ?assertNot(maps:is_key(<<"slot">>, Converted)),
    ?assertMatch({ok, _}, wfcli_build_plan:request(Group#{
                    <<"options">> => #{<<"preserve_source_slots">> => true}})),
    ?assertMatch({ok, _}, wfcli_build_plan:request(Group#{
                    <<"baseline">> => (baseline())#{<<"class">> => <<"archwing">>}})).

group() ->
    #{<<"id">> => <<"group-1">>, <<"revision">> => 4,
      <<"options">> => #{<<"preserve_source_slots">> => true,
                           <<"allow_omni">> => false,
                           <<"allow_umbral_forma">> => false},
      <<"baseline">> => baseline(),
      <<"members">> => [player_member(), source_member()]}.

baseline() ->
    #{<<"instance_id">> => <<"copy-1">>,
      <<"definition_id">> => <<"/warframe">>,
      <<"class">> => <<"warframe">>, <<"capacity">> => 30,
      <<"features">> => #{<<"double_capacity">> => true},
      <<"effective_polarities">> =>
          [#{<<"slot_id">> => <<"mod-1">>, <<"player_index">> => 7,
             <<"polarity">> => <<"madurai">>},
           #{<<"slot_id">> => <<"mod-2">>, <<"player_index">> => 6,
             <<"polarity">> => <<"none">>},
           #{<<"slot_id">> => <<"aura">>, <<"player_index">> => 8,
             <<"polarity">> => <<"madurai">>}],
      <<"shard_slots">> =>
          [#{<<"slot_id">> => <<"shard-1">>,
             <<"upgrade">> => #{<<"upgrade_type">> => <<"crimson">>}}],
      <<"topology">> =>
          #{<<"regions">> =>
                [#{<<"slots">> =>
                       [slot(<<"aura">>, 8, 9, <<"aura">>, true),
                        slot(<<"exilus">>, 9, 10, <<"exilus">>, true)]},
                 #{<<"slots">> =>
                       [slot(<<"mod-1">>, 7, 1, <<"mod">>, true),
                        slot(<<"mod-2">>, 6, 2, <<"mod">>, true)]},
                 #{<<"slots">> =>
                       [slot(<<"arcane-1">>, 10, 11, <<"arcane">>, false)]}]}}.

slot(Id, Index, External, Role, Planner) ->
    #{<<"id">> => Id, <<"player_index">> => Index,
      <<"build_slot">> => External, <<"role">> => Role,
      <<"label">> => Id, <<"planner">> => Planner}.

player_member() ->
    #{<<"id">> => <<"config:1">>, <<"kind">> => <<"player_config">>,
      <<"name">> => <<"Roar config">>,
      <<"snapshot">> =>
          #{<<"config">> =>
                #{<<"ability_override">> => [<<"/ability/roar">>],
                  <<"upgrade_slots">> =>
                      [#{<<"slot">> => 7, <<"name">> => <<"Vitality">>,
                         <<"polarity">> => <<"vazarin">>, <<"base_drain">> => 2,
                         <<"rank">> => 6},
                       #{<<"slot">> => 10, <<"name">> => <<"Arcane Energize">>,
                         <<"role">> => <<"arcane">>, <<"rank">> => 5}]}}}.

source_member() ->
    Slot = #{<<"source_slot">> => 2, <<"topology_slot">> => <<"mod-2">>,
             <<"name">> => <<"Redirection">>, <<"kind">> => <<"mod">>,
             <<"mod_polarity">> => <<"vazarin">>,
             <<"cost">> => 8, <<"rank">> => 5},
    #{<<"id">> => <<"source:1">>, <<"kind">> => <<"source_revision">>,
      <<"name">> => <<"Source build">>,
      <<"snapshot">> =>
          #{<<"content">> =>
                #{<<"slots">> => [maps:remove(<<"topology_slot">>, Slot)]},
            <<"presentation">> => #{<<"upgrades">> => [Slot]}}}.

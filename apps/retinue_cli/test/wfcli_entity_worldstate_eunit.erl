%%%-------------------------------------------------------------------
%% EUnit tests for worldstate entity builder.
%%%-------------------------------------------------------------------
-module(wfcli_entity_worldstate_eunit).

-include_lib("eunit/include/eunit.hrl").

optional_fields_indexed_test() ->
    Data = #{<<"Foo">> => <<"Value">>, <<"Baz">> => 2},
    Entry = wfcli_entity_worldstate:build(meta, "meta_1", "Meta", Data, #{resolve_items => false}),
    Extras = maps:get(extra_fields, Entry, #{}),
    ?assertEqual("Value", maps:get("Foo", Extras)),
    ?assertEqual("2", maps:get("Baz", Extras)),
    Fields = maps:get(fields, Entry, []),
    ?assert(lists:member("Value", Fields)),
    ?assert(lists:member("2", Fields)).

optional_nested_indexed_test() ->
    Data = #{
        <<"Listy">> => [<<"Alpha">>, <<"Beta">>],
        <<"Nested">> => #{<<"Inner">> => <<"Gamma">>}
    },
    Entry = wfcli_entity_worldstate:build(alert, "alert_2", "Alert", Data, #{resolve_items => false}),
    Extras = maps:get(extra_fields, Entry, #{}),
    ?assertEqual(undefined, maps:get("Listy", Extras, undefined)),
    ?assertEqual(undefined, maps:get("Nested", Extras, undefined)),
    Haystack = maps:get(haystack, Entry, ""),
    ?assert(string:find(Haystack, "alpha") =:= nomatch),
    ?assert(string:find(Haystack, "gamma") =:= nomatch),
    RawEntry = wfcli_entity_worldstate:build(alert, "alert_2", "Alert", Data, #{resolve_items => false, search_raw => true}),
    RawHaystack = maps:get(haystack, RawEntry, ""),
    ?assert(string:find(RawHaystack, "alpha") =/= nomatch),
    ?assert(string:find(RawHaystack, "gamma") =/= nomatch).

extra_field_resolves_mission_type_test() ->
    Data = #{<<"MissionType">> => <<"MT_DEFENSE">>},
    Entry = wfcli_entity_worldstate:build(meta, "meta_3", "Meta", Data, #{resolve_items => true}),
    Extras = maps:get(extra_fields, Entry, #{}),
    ?assertEqual("Defense", maps:get("MissionType", Extras, "")).

extra_field_resolves_faction_suffix_test() ->
    Data = #{<<"AttackerFaction">> => <<"FC_CORPUS">>},
    Entry = wfcli_entity_worldstate:build(meta, "meta_4", "Meta", Data,
                                          #{resolve_items => true}),
    Extras = maps:get(extra_fields, Entry, #{}),
    ?assertEqual("Corpus", maps:get("AttackerFaction", Extras, "")).

resolve_any_modifier_test() ->
    ?assertEqual("Meso", wfcli_resolve:resolve("any", "VoidT2", #{resolve_items => true})).

extra_fields_skip_null_values_test() ->
    Data = #{<<"Foo">> => <<"Bar">>, <<"Baz">> => null},
    Entry = wfcli_entity_worldstate:build(meta, "meta_5", "Meta", Data, #{resolve_items => false}),
    Extras = maps:get(extra_fields, Entry, #{}),
    ?assertEqual("Bar", maps:get("Foo", Extras)),
    ?assertEqual(undefined, maps:get("Baz", Extras, undefined)).

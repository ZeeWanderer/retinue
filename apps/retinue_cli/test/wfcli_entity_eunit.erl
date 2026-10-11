%%%-------------------------------------------------------------------
%% EUnit tests for generic entity builder.
%%%-------------------------------------------------------------------
-module(wfcli_entity_eunit).

-include_lib("eunit/include/eunit.hrl").

extra_fields_skip_nested_test() ->
    Data = #{
        <<"Name">> => <<"Alpha">>,
        <<"Count">> => 3,
        <<"Nested">> => #{<<"Inner">> => <<"Skip">>},
        <<"Listy">> => [<<"One">>, <<"Two">>]
    },
    Spec = #{row_map_fun => fun(_Entry, _Opts) -> #{name => "Alpha"} end},
    Entry = wfcli_entity:build(test, "id1", "Alpha", Data, #{}, Spec),
    Extras = maps:get(extra_fields, Entry, #{}),
    ?assertEqual("3", maps:get("Count", Extras)),
    ?assertEqual(undefined, maps:get("Nested", Extras, undefined)),
    ?assertEqual(undefined, maps:get("Listy", Extras, undefined)).

resolved_strings_in_haystack_test() ->
    Data = #{<<"Name">> => <<"Alpha">>},
    Spec = #{
        row_map_fun => fun(_Entry, _Opts) -> #{name => "Alpha"} end,
        resolve_strings_fun => fun(_Str, _Opts) -> "Resolved" end
    },
    Entry = wfcli_entity:build(test, "id2", "Alpha", Data, #{search_raw => true}, Spec),
    Haystack = maps:get(haystack, Entry, ""),
    ?assert(string:find(Haystack, "resolved") =/= nomatch).

raw_strings_excluded_from_default_haystack_test() ->
    Data = #{<<"Hidden">> => #{<<"Name">> => <<"Duviri">>}},
    Spec = #{row_map_fun => fun(_Entry, _Opts) -> #{name => "Visible"} end},
    Entry = wfcli_entity:build(test, "id4", "Visible", Data, #{}, Spec),
    Haystack = maps:get(haystack, Entry, ""),
    ?assert(string:find(Haystack, "visible") =/= nomatch),
    ?assert(string:find(Haystack, "duviri") =:= nomatch).

collect_strings_includes_numbers_test() ->
    Data = #{<<"Count">> => 42},
    Strings = wfcli_entity:collect_strings(Data),
    ?assert(lists:member("42", Strings)).

extra_fields_skip_system_keys_test() ->
    Data = #{<<"_id">> => <<"skip">>, <<"id">> => <<"skip2">>, <<"Name">> => <<"Alpha">>},
    Spec = #{row_map_fun => fun(_Entry, _Opts) -> #{name => "Alpha"} end},
    Entry = wfcli_entity:build(test, "id3", "Alpha", Data, #{}, Spec),
    Extras = maps:get(extra_fields, Entry, #{}),
    ?assertEqual(undefined, maps:get("_id", Extras, undefined)),
    ?assertEqual(undefined, maps:get("id", Extras, undefined)).

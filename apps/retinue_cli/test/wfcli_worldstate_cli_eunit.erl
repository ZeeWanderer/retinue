%%%-------------------------------------------------------------------
%% EUnit tests for worldstate CLI watch helpers.
%%%-------------------------------------------------------------------
-module(wfcli_worldstate_cli_eunit).

-include_lib("eunit/include/eunit.hrl").

split_signature_merge_tail_test() ->
    Parts = wfcli_worldstate_output:split_signature("A | B | C", 2),
    ?assertEqual(["A", "B | C"], Parts).

removed_entries_table_builds_row_map_test() ->
    Diff = #{removed => [{"id1", #{name => "Name", value => "X | Y"}}]},
    Columns = [one, two],
    [Entry] = wfcli_worldstate_output:removed_entries_table(Diff, Columns),
    RowMap = maps:get(row_map, Entry, #{}),
    ?assertEqual("X", maps:get(one, RowMap)),
    ?assertEqual("Y", maps:get(two, RowMap)),
    ?assertEqual("id1", maps:get(id, Entry)).

removed_extract_rows_test() ->
    Diff = #{removed => [{"id1", #{name => "Name", value => "X | Y"}}]},
    Rows = wfcli_worldstate_output:removed_extract_rows(Diff, ["one", "two"]),
    ?assertEqual([["Name", "X", "Y"]], Rows).

merge_inline_entries_keeps_removed_order_test() ->
    PrevSnap = #{},
    Opts = #{raw => true},
    ExpB = retinue_time:format_millis(2000, Opts),
    Entries = [
        #{id => "A", name => "A", type => alert, row_map => #{expiry => retinue_time:format_millis(1000, Opts)}, data => #{
            <<"Expiry">> => 1000,
            <<"MissionInfo">> => #{<<"missionType">> => <<"MT_CAPTURE">>}
        }},
        #{id => "C", name => "C", type => alert, row_map => #{expiry => retinue_time:format_millis(3000, Opts)}, data => #{
            <<"Expiry">> => 3000,
            <<"MissionInfo">> => #{<<"missionType">> => <<"MT_CAPTURE">>}
        }}
    ],
    Diff = #{removed => [{"B", #{name => "B", row_map => #{expiry => ExpB}}}]},
    Columns = [expiry, mission, node, reward],
    Merged = wfcli_worldstate_output:merge_inline_entries(Entries, Columns, Diff, PrevSnap, Opts),
    Names = [maps:get(name, E, "") || E <- Merged],
    ?assertEqual(["A", "B", "C"], Names).

opaque_queries_are_not_validated_by_cli_test() ->
    lists:foreach(fun(Query) ->
        {ok, Parsed} = wfcli_test_cli:worldstate(["alerts", "--search", Query]),
        ?assertEqual(Query, maps:get(search, Parsed)),
        ?assertNot(maps:get(watch, Parsed))
    end, ["foo OR", "faction=Corpus", "foo OR bar"]).

archimedea_selector_adds_semantic_filter_test() ->
    {ok, Parsed} = wfcli_test_cli:worldstate(
                     ["archimedea", "--deep", "--search", "risk~shielded"]),
    ?assertEqual(archimedea, maps:get(type_filter, Parsed)),
    ?assertEqual("(risk~shielded) archimedea=deep", maps:get(search, Parsed)).

scoped_commands_match_option_forms_test() ->
    {ok, Baro} = wfcli_test_cli:worldstate(["baro", "inventory"]),
    ?assertEqual(baro, maps:get(type_filter, Baro)),
    ?assertEqual(true, maps:get(inventory, Baro)),
    ?assertNot(maps:get(watch, Baro)),
    {ok, Temporal} = wfcli_test_cli:worldstate(["archimedea", "temporal"]),
    ?assertEqual("archimedea=temporal", maps:get(search, Temporal)).

archimedea_selectors_are_exclusive_test() ->
    ?assertMatch({error, _}, wfcli_test_cli:worldstate(["archimedea", "--deep", "--temporal"])),
    ?assertMatch({error, _}, wfcli_test_cli:worldstate(["archimedea", "deep", "--temporal"])).

watch_spec_defers_bad_query_syntax_test() ->
    {ok, Parsed} = wfcli_test_cli:worldstate(["watch", "--spec", "alerts:foo OR"]),
    [Spec] = maps:get(watch_specs, Parsed),
    ?assertEqual("foo OR", maps:get(query, Spec)).

watch_modes_and_order_test() ->
    lists:foreach(fun(Flags) ->
        {ok, Parsed} = wfcli_test_cli:worldstate(["fissures" | Flags]),
        ?assert(maps:get(watch, Parsed))
    end, [["--watch"], ["--diff"], ["--diff-style", "inline"], ["--always"]]),
    {ok, Parsed} = wfcli_test_cli:worldstate(
                     ["watch", "--spec", "alerts:endo", "--", "fissures:lith", "calendar"]),
    ?assertEqual([alert, fissure, calendar],
                 [maps:get(type_filter, S) || S <- lists:reverse(maps:get(watch_specs, Parsed))]),
    ?assertMatch({error, _}, wfcli_test_cli:worldstate(["baro", "inventory", "--watch"])).

watch_table_includes_extra_columns_test() ->
    RowMaps = [#{mission => "Capture", extra_fields => #{"Icon" => "icon.png"}}],
    Cols = [mission],
    Opts = #{watch_table => true},
    ?assertEqual([{extra, "Icon"}], wfcli_worldstate_output:maybe_extra_columns(RowMaps, Cols, Opts)).

removed_entry_preserves_extra_fields_test() ->
    Prev = #{row_map => #{mission => "Capture"}, extra_fields => #{"Icon" => "icon.png"}},
    Columns = [mission, {extra, "Icon"}],
    Diff = #{removed => [{"k1", Prev}]},
    [Entry] = wfcli_worldstate_output:removed_entries_table(Diff, Columns),
    ?assertEqual(#{"Icon" => "icon.png"}, maps:get(extra_fields, Entry)),
    RowMap = maps:get(row_map, Entry),
    ?assertEqual("Capture", maps:get(mission, RowMap)).

regular_table_includes_extra_columns_test() ->
    RowMaps = [#{mission => "Capture", extra_fields => #{"Icon" => "icon.png"}}],
    Cols = [mission],
    ?assertEqual([{extra, "Icon"}], wfcli_worldstate_output:maybe_extra_columns(RowMaps, Cols, #{})).

daemon_memory_source_text_test() ->
    Result = #{source => memory, snapshot_origin => fetched, snapshot_age_ms => 24500},
    ?assertEqual("source: memory, origin: fetched, age: 24s",
                 wfcli_worldstate_output:daemon_source_text(Result)).

daemon_fetched_source_text_avoids_duplicate_origin_test() ->
    Result = #{source => fetched, snapshot_origin => fetched, snapshot_age_ms => 12},
    ?assertEqual("source: fetched, age: 0s",
                 wfcli_worldstate_output:daemon_source_text(Result)).

help_is_scoped_to_real_arguments_test() ->
    ?assertEqual([], wfcli_test_cli:options(["teshin"]) --
                     wfcli_test_cli:options(["query"])),
    ?assertNot(lists:member("--refresh", wfcli_test_cli:options(["teshin"]))),
    ?assertNot(lists:member("--inventory", wfcli_test_cli:options(["alerts"]))),
    ?assert(lists:member("--watch", wfcli_test_cli:options(["alerts"]))),
    ?assert(lists:member("--inventory", wfcli_test_cli:options(["baro"]))),
    ?assert(lists:member("--day", wfcli_test_cli:options(["calendar"]))),
    ?assertEqual(block, maps:get(output_format, wfcli_test_cli:parse(["archimedea"]))).

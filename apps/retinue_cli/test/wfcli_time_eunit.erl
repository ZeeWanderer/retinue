%%%-------------------------------------------------------------------
%% EUnit tests for shared time helpers.
%%%-------------------------------------------------------------------
-module(wfcli_time_eunit).

-include_lib("eunit/include/eunit.hrl").

format_millis_explicit_utc_test() ->
    Output = retinue_time:format_millis(1701430200000, #{utc => true}),
    ?assert(re:run(Output, "Z$", []) =/= nomatch).

format_millis_local_offset_test() ->
    Output = retinue_time:format_millis(1701430200000, #{raw => false}),
    ?assert(re:run(Output, "Z$", []) =:= nomatch),
    ?assert(re:run(Output, "[+-][0-9]{2}:[0-9]{2}$", []) =/= nomatch).

format_millis_string_input_test() ->
    Output = retinue_time:format_millis("1701430200000", #{raw => false}),
    ?assert(re:run(Output, "[+-][0-9]{2}:[0-9]{2}$", []) =/= nomatch).

format_millis_passthrough_test() ->
    ?assertEqual("soon", retinue_time:format_millis("soon", #{raw => false})).

json_timestamps_are_utc_test() ->
    ?assertEqual("2023-12-01T10:10:00.123Z",
        retinue_time:format_millis(1701425400123, #{output_format => json, precision => millisecond})).

raw_identifiers_do_not_change_timezone_test() ->
    ?assertEqual(retinue_time:format_millis(1000), retinue_time:format_millis(1000, #{raw => true})).

parse_instants_test() ->
    ?assertEqual({ok, 1000}, retinue_time:parse("1970-01-01T00:00:01Z")),
    ?assertEqual({ok, 1234}, retinue_time:parse(<<"1970-01-01t02:00:01.234+02:00">>)),
    ?assertEqual({ok, 1234}, retinue_time:parse("1969-12-31T19:00:01.234-05:00")),
    ?assertEqual({ok, -1000}, retinue_time:parse("-1000")),
    ?assertEqual({ok, 1234}, retinue_time:parse(1234)).

relative_times_test() ->
    ?assertEqual({ok, 1000}, retinue_time:parse("now", 1000)),
    lists:foreach(fun({Unit, Size}) ->
        ?assertEqual({ok, 1000 - 2 * Size}, retinue_time:parse("now-2" ++ Unit, 1000)),
        ?assertEqual({ok, 1000 + Size}, retinue_time:parse("now+1" ++ Unit, 1000))
    end, [{"ms", 1}, {"s", 1000}, {"m", 60000}, {"h", 3600000},
          {"d", 86400000}, {"w", 604800000}]).

invalid_times_test() ->
    lists:foreach(fun(Value) -> ?assertEqual(error, retinue_time:parse(Value)) end,
        ["yesterday", "now-1", "now+1x", "2026-02-30T00:00:00Z",
         "2026-13-01T00:00:00Z", "2026-01-01T24:00:00Z",
         "2026-01-01T00:00:00+25:00", "2026-01-01T00:00:00+01:60",
         "2026-01-01T00:00:00", "2026-01-01", "", null, undefined]).

rfc3339_display_uses_selected_timezone_test() ->
    ?assertEqual("1970-01-01T00:00:01Z",
                 retinue_time:format_millis("1970-01-01T02:00:01+02:00", #{utc => true})).

-module(wfcli_diagnostics_cli_eunit).

-include_lib("eunit/include/eunit.hrl").

log_commands_parse_sources_and_limits_test() ->
    Args = wfcli_test_cli:parse(["diagnostics", "logs", "companion", "--lines", "7", "--json"]),
    ?assertMatch(#{source := companion, lines := 7, output_format := json}, Args),
    ?assertMatch(#{source := companion, lines := 3},
                 wfcli_test_cli:parse(["companion", "logs", "--lines", "3"])),
    ?assertMatch({error, _, _, _}, wfcli_test_cli:result(["diagnostics", "logs", "--lines", "0"])),
    ?assertMatch({error, _, _, _}, wfcli_test_cli:result(["diagnostics", "logs", "--lines", "1001"])),
    ?assertMatch({ok, _, _, _}, wfcli_test_cli:result(["companion", "capture", "status"])).

log_and_capture_completion_test() ->
    Sources = wfcli_test_cli:candidates(["diagnostics", "logs", ""]),
    ?assert(lists:member("daemon", Sources)),
    ?assert(lists:member("companion", Sources)),
    ?assert(lists:member("--lines", Sources)),
    ?assert(lists:member("status", wfcli_test_cli:candidates(["companion", "capture", ""]))),
    ?assertMatch(#{source := all}, wfcli_test_cli:parse(["diagnostics", "logs"])).

log_tail_preserves_plain_and_malformed_entries_test() ->
    Event = #{<<"timestamp_ms">> => 1000, <<"level">> => <<"warn">>,
              <<"event">> => <<"metadata.failed">>, <<"message">> => <<"missing type">>},
    with_log(["old\n", jsone:encode(Event), "\n{incomplete\nplain daemon warning\n"],
        fun(Path) ->
            Report = wfcli_diagnostics_cli:read_log(companion, Path, 3),
            ?assertEqual([Event, #{<<"message">> => <<"{incomplete">>},
                                #{<<"message">> => <<"plain daemon warning">>}], maps:get(entries, Report)),
            Rendered = retinue_output:with(#{utc => true}, fun() ->
                unicode:characters_to_binary(wfcli_diagnostics_format:format_log(Report))
            end),
            ?assertNotEqual(nomatch, binary:match(Rendered, <<"1970-01-01T00:00:01.000Z warn metadata.failed: missing type">>))
        end).

log_read_is_bounded_and_drops_partial_first_line_test() ->
    with_log([binary:copy(<<"x">>, 2 * 1024 * 1024), "\nlast\n"],
        fun(Path) ->
            ?assertMatch(#{entries := [#{<<"message">> := <<"last">>}]},
                         wfcli_diagnostics_cli:read_log(daemon, Path, 50))
        end).

missing_and_empty_logs_are_not_errors_test() ->
    with_log(<<>>, fun(Path) ->
        ?assertMatch(#{entries := []}, wfcli_diagnostics_cli:read_log(daemon, Path, 5)),
        ok = file:delete(Path),
        ?assertMatch(#{entries := [], missing := true},
                     wfcli_diagnostics_cli:read_log(daemon, Path, 5))
    end).

unexpected_json_shape_remains_readable_test() ->
    with_log(<<"{\"event\":null,\"message\":42}\n">>, fun(Path) ->
        #{entries := [Entry]} = wfcli_diagnostics_cli:read_log(companion, Path, 1),
        ?assertEqual(#{<<"message">> => <<"{\"event\":null,\"message\":42}">>}, Entry)
    end).

legacy_daemon_lines_have_queryable_timestamps_test() ->
    with_log(<<"2026-09-30T15:00:00+03:00 warning: adapter missing\n">>, fun(Path) ->
        {ok, Millis} = retinue_time:parse("2026-09-30T12:00:00Z"),
        ?assertMatch(#{entries := [#{<<"timestamp_ms">> := Millis,
                                     <<"message">> := <<"adapter missing">>}]},
                     wfcli_diagnostics_cli:read_log(daemon, Path, 1))
    end).

with_log(Content, Test) ->
    Path = "/tmp/wfcli-incidents-" ++ integer_to_list(erlang:unique_integer([positive])),
    ok = file:write_file(Path, Content),
    try Test(Path) after file:delete(Path) end.

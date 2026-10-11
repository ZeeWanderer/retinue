%%%-------------------------------------------------------------------
%% EUnit tests for worldstate watch helpers.
%%%-------------------------------------------------------------------
-module(wf_worldstate_watch_eunit).

-include_lib("eunit/include/eunit.hrl").

diff_and_format_test() ->
    Prev = #{
        "a" => #{name => "A", value => "1"},
        "b" => #{name => "B", value => "2"}
    },
    Curr = #{
        "b" => #{name => "B", value => "3"},
        "c" => #{name => "C", value => "4"}
    },
    Diff = wf_worldstate_watch:diff(Prev, Curr),
    ?assert(wf_worldstate_watch:has_changes(Diff)),
    Lines = wf_worldstate_watch:format_diff_lines(Diff, true),
    LinesMin = wf_worldstate_watch:format_diff_lines_min(Diff, true),
    StatusMap = wf_worldstate_watch:diff_status_map(Diff),
    Stripped = [retinue_tty:strip_ansi(L) || L <- Lines],
    StrippedMin = [retinue_tty:strip_ansi(L) || L <- LinesMin],
    ?assertEqual(add, maps:get("c", StatusMap)),
    ?assertEqual(remove, maps:get("a", StatusMap)),
    ?assertEqual(change, maps:get("b", StatusMap)),
    ?assert(lists:any(fun(L) -> lists:prefix("added ", L) end, Stripped)),
    ?assert(lists:any(fun(L) -> lists:prefix("removed ", L) end, Stripped)),
    ?assert(lists:any(fun(L) -> lists:prefix("changed ", L) end, Stripped)),
    ?assert(lists:all(fun(L) -> not lists:prefix("added ", L) andalso
                               not lists:prefix("removed ", L) andalso
                               not lists:prefix("changed ", L) end, StrippedMin)),
    ?assert(lists:all(fun(L) -> not lists:prefix("+", L) andalso not lists:prefix("-", L) end, Stripped)).

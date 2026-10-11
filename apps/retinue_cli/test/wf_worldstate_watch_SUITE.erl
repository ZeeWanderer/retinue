%%%-------------------------------------------------------------------
%% Common Test for worldstate watch helpers.
%%%-------------------------------------------------------------------
-module(wf_worldstate_watch_SUITE).

-export([all/0,
         diff_and_format/1]).
-include_lib("common_test/include/ct.hrl").
-include_lib("eunit/include/eunit.hrl").

all() ->
    [diff_and_format].

diff_and_format(_Config) ->
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
    Stripped = [retinue_tty:strip_ansi(L) || L <- Lines],
    ?assert(lists:any(fun(L) -> lists:prefix("added ", L) end, Stripped)),
    ?assert(lists:any(fun(L) -> lists:prefix("removed ", L) end, Stripped)),
    ?assert(lists:any(fun(L) -> lists:prefix("changed ", L) end, Stripped)),
    ?assert(lists:all(fun(L) -> not lists:prefix("+", L) andalso not lists:prefix("-", L) end, Stripped)).

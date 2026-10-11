-module(wfcli_circuit_eunit).
-include_lib("eunit/include/eunit.hrl").
-include_lib("retinue_daemon/include/wfcli_worldstate.hrl").

active_week_is_selected_at_query_time_test() ->
    {ok, Bin} = file:read_file("apps/retinue_cli/test/fixtures/circuit_schedule.json"),
    Raw = jsone:decode(Bin),
    Start = maps:get(<<"Time">>, Raw),
    End = Start + 7 * 24 * 60 * 60,
    Opts = #{raw => true, resolve_items => false},
    Ws = wfcli_worldstate:reindex(#ws{raw = Raw}, Opts),
    ?assertEqual(4, length([E || E = #{type := circuit} <- wfcli_worldstate:index(Ws)])),
    ?assertEqual([], entries_at(Ws, Start - 1)),
    [Normal, Steel] = entries_at(Ws, Start),
    ?assertEqual([Normal, Steel], entries_at(Ws, End - 1)),
    ?assertEqual("Normal", maps:get(name, Normal)),
    ?assertEqual("Steel Path", maps:get(name, Steel)),
    ?assertEqual([<<"Garuda">>, <<"Baruuk">>, <<"Hildryn">>], choices(Normal)),
    ?assertEqual(5, length(choices(Steel))),
    [NextNormal, NextSteel] = entries_at(Ws, End),
    ?assertEqual([<<"Rhino">>, <<"Excalibur">>, <<"Mag">>], choices(NextNormal)),
    ?assertEqual([<<"Braton">>, <<"Lato">>], choices(NextSteel)),
    ?assertEqual([], entries_at(Ws, End + 7 * 24 * 60 * 60)).

category_query_and_alias_test() ->
    ?assertEqual(circuit, wfcli_worldstate_schema:type_from_label("Circuit")),
    ?assertEqual(circuit, wfcli_worldstate_schema:type_from_label("Endless XP")),
    ?assert(lists:member("circuit", wfcli_cli:public_command_names())),
    ?assertNot(lists:member("endless-xp", wfcli_cli:public_command_names())),
    lists:foreach(fun(Command) ->
        {ok, Parsed} = wfcli_test_cli:worldstate([Command, "steel-path"]),
        ?assertEqual(circuit, maps:get(type_filter, Parsed)),
        ?assertEqual("data.Category=EXC_HARD", maps:get(search, Parsed))
    end, ["circuit", "endless-xp"]),
    ?assertMatch({error, _}, wfcli_test_cli:worldstate(["circuit", "normal", "steel-path"])).

entries_at(Ws, Now) ->
    wfcli_worldstate_results:entries(Ws#ws{opts = (Ws#ws.opts)#{now_fun => fun() -> Now end}},
                                     circuit, undefined).

choices(Entry) -> maps:get(<<"Choices">>, maps:get(data, Entry)).

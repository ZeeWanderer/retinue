-module(retinue_doctor_eunit).
-include_lib("eunit/include/eunit.hrl").

commands_and_aliases_test() ->
    ?assertMatch({ok, #{game := all}, _, _}, retinue_cli_args:parse(["doctor"])),
    ?assertMatch({ok, #{game := poe2, output_format := json}, _, _},
                 retinue_cli_args:parse(["doctor", "poe2", "--json"])),
    ?assertMatch({ok, #{game := warframe}, _, _},
                 retinue_cli_args:parse(["warframe", "doctor"])),
    lists:foreach(fun(Path) ->
        {ok, _, _, New} = retinue_cli_args:parse(["warframe", "diagnostics" | Path]),
        {ok, _, _, Old} = retinue_cli_args:parse(["warframe", "companion" | Path]),
        ?assert(maps:is_key(handler, New)),
        ?assert(maps:is_key(handler, Old))
    end, [["capture", "status"], ["probe"], ["preview", "list"]]),
    ?assertMatch({ok, #{game := poe2, all := true}, _, _},
                 retinue_cli_args:parse(["poe2", "diagnostics", "incidents", "--all"])),
    ?assert(lists:member("poe2", retinue_completion:candidates(["doctor", ""], retinue_cli:command()))),
    ?assert(lists:member("capture", retinue_completion:candidates(["warframe", "diagnostics", ""], retinue_cli:command()))).

inactive_is_not_healthy_test() ->
    Report = report(warframe, info([]), not_requested),
    ?assertEqual(incomplete, maps:get(status, Report)),
    ?assertEqual(0, maps:get(failures, Report)),
    ?assertEqual(failed, maps:get(status, report(all, maps:remove(protocols, info([])), not_requested))),
    ?assertMatch(#{status := failed}, retinue_doctor:report(all, {stopped, node()}, not_requested, history([]), 100000)).

connection_alone_does_not_verify_adapters_test() ->
    Report = report(warframe, info([client(warframe)]), {ok, #{<<"snapshot">> => snapshot()}}),
    ?assertEqual(incomplete, maps:get(status, Report)),
    ?assertEqual(11, maps:get(unverified, Report)),
    ?assertNotEqual(nomatch, binary:match(iolist_to_binary(retinue_doctor:format(Report)),
        <<"11 checks have not run or are waiting for the game.">>)).

unresolved_incidents_survive_sessions_and_versions_test() ->
    Open = incident(<<"warframe.companion">>, <<"open">>),
    Check = fun(Entries) -> retinue_doctor:report(warframe, {running, node(), info([])},
                                                not_requested, history(Entries), 100000) end,
    ?assertMatch(#{status := failed, failures := 1}, Check([Open])),
    Old = Open#{<<"state">> := <<"unverified">>,
                <<"context">> := #{<<"version">> => <<"0.1.0">>, <<"session">> => <<"previous">>}},
    Report = Check([Old]),
    ?assertMatch(#{status := failed, failures := 1}, Report),
    [Recorded] = [C || #{kind := incident} = C <- maps:get(checks, Report)],
    ?assertMatch(#{context := #{<<"version">> := <<"0.1.0">>}, observed_at := {timestamp, 99000}}, Recorded),
    ?assertNotEqual(nomatch, binary:match(maps:get(message, Recorded), <<"no successful rerun recorded">>)),
    ?assertMatch(#{status := incomplete, failures := 0}, Check([Old#{<<"last_result">> := <<"resolved">>}])),
    ?assertMatch(#{status := incomplete, failures := 0}, Check([incident(<<"poe2.companion">>, <<"open">>)])),
    ?assertMatch(#{status := failed}, retinue_doctor:report(all, {running, node(), info([])},
        not_requested, history([incident(<<"poe2.companion">>, <<"open">>)]), 100000)).

unavailable_history_is_not_a_clean_bill_of_health_test() ->
    Info = {running, node(), info([client(warframe)])},
    Live = live((snapshot())#{<<"adapters">> => adapters()}),
    lists:foreach(fun(History) ->
        ?assertMatch(#{status := failed, failures := 1},
            retinue_doctor:report(warframe, Info, Live, History, 100000))
    end, [{error, checks_unavailable}, {ok, #{checks => [], storage_error => invalid_check_store}}]).

live_failure_and_recovery_test() ->
    Info = info([client(warframe)]),
    Snapshot = (snapshot())#{<<"adapters">> => adapters()},
    ?assertEqual(passed, maps:get(status, report(warframe, Info, live(Snapshot)))),
    ?assertEqual(incomplete, maps:get(status, report(warframe, Info, live(Snapshot#{
        <<"game_metadata_source">> => <<"cache">>})))),
    Failed = (adapters())#{<<"relic_rewards">> => evidence(<<"fail">>, <<"layout changed">>)},
    ?assertEqual(failed, maps:get(status, report(warframe, Info, live(Snapshot#{<<"adapters">> := Failed})))),
    ?assertEqual(failed, maps:get(status, report(warframe, Info, live(Snapshot#{<<"companion_pid">> := 99})))),
    Stale = (adapters())#{<<"native_inventory">> => (evidence(<<"pass">>, null))#{<<"checked_at">> := 1}},
    ?assertEqual(failed, maps:get(status, report(warframe, Info, live(Snapshot#{<<"adapters">> := Stale})))),
    ?assertEqual(failed, maps:get(status, report(warframe, Info, {error, companion_diagnostic_timeout}))).

poe2_cached_and_live_sample_test() ->
    Current = #{<<"scene">> => <<"in_game">>, <<"area">> => #{<<"name">> => <<"Town">>},
                <<"player">> => #{<<"name">> => <<"Player">>},
                <<"panels">> => #{<<"inventory">> => false}, <<"issues">> => [],
                <<"social">> => #{<<"local">> => null, <<"party">> => []},
                <<"entities">> => #{<<"count">> => 1, <<"unique_monsters">> => []}},
    Data = #{<<"status">> => <<"observed">>, <<"current">> => Current, <<"observed_at">> => 99999},
    Info = info([client(poe2)]),
    Check = fun(D) -> report(poe2, Info#{games => #{poe2 => D}}, not_requested) end,
    ?assertEqual(incomplete, maps:get(status, Check(Data))),
    Live = Data#{<<"receipt">> => #{<<"connected">> => true, <<"received_at">> => 99999}},
    ?assertEqual(passed, maps:get(status, Check(Live))),
    ?assertEqual(failed, maps:get(status, Check(Live#{<<"observed_at">> := 1}))),
    ?assertEqual(incomplete, maps:get(status, Check(Live#{<<"current">> := Current#{
        <<"panels">> := #{<<"inventory">> => null}}}))),
    ?assertEqual(incomplete, maps:get(status, Check(Live#{<<"current">> := maps:remove(<<"social">>, Current)}))),
    SocialError = [#{<<"field">> => <<"social">>, <<"reason">> => <<"invalid vector">>}],
    SocialChecks = maps:get(checks, Check(Live#{<<"current">> := Current#{
        <<"social">> := null, <<"issues">> := SocialError}})),
    ?assertMatch([#{state := fail}], [C || #{label := <<"Social memory">>} = C <- SocialChecks]),
    GoldError = [#{<<"field">> => <<"gold">>, <<"reason">> => <<"invalid pointer">>}],
    ?assertEqual(failed, maps:get(status, Check(Live#{<<"current">> := Current#{<<"issues">> := GoldError}}))),
    ?assertEqual(failed, maps:get(status, Check(Live#{<<"receipt">> := #{<<"connected">> => true, <<"received_at">> => 1}}))).

report(Game, Info, Live) -> retinue_doctor:report(Game, {running, node(), Info}, Live, history([]), 100000).
history(Checks) -> {ok, #{checks => Checks, storage_error => undefined}}.
incident(Component, Result) ->
    #{<<"component">> => Component, <<"check">> => <<"reader">>, <<"state">> => Result,
      <<"last_result">> => Result, <<"error">> => <<"anchor not found">>, <<"checked_at">> => 99000,
      <<"context">> => #{<<"version">> => <<"0.2.1">>}}.
info(Clients) -> #{version => "0.2.1", local_api => #{clients => Clients}, games => #{},
                  protocols => #{erlang_distribution_rpc => retinue_protocol:contract()}}.
client(Game) -> #{game => atom_to_binary(Game), client => case Game of
                    warframe -> <<"wfcompanion">>; poe2 -> <<"poe2companion">> end,
                  mode => <<"launch">>, os_pid => 42, version => <<"0.2.1">>}.
live(Snapshot) -> {ok, #{<<"snapshot">> => Snapshot}}.
snapshot() -> #{<<"companion_pid">> => 42, <<"game_pid">> => 84,
                <<"debug_output_active">> => true, <<"debug_output_lines_observed">> => 10,
                <<"inventory_active">> => true, <<"inventory_updates_observed">> => 1,
                <<"game_metadata_updates_observed">> => 1}.
adapters() -> maps:from_list([{K, evidence(<<"pass">>, null)} || K <-
    [<<"ui_bindings">>, <<"http_inventory">>, <<"native_inventory">>, <<"account">>,
     <<"relic_selection">>, <<"relic_rewards">>, <<"ducat_bindings">>,
     <<"ducat_kiosk">>, <<"ducat_tooltip">>, <<"ducat_inventory">>, <<"ducat_market">>]]).
evidence(State, Error) -> #{<<"state">> => State, <<"error">> => Error, <<"checked_at">> => 99999}.

-module(retinue_architecture_eunit).
-include_lib("eunit/include/eunit.hrl").

module_names_follow_ownership_test() ->
    lists:foreach(fun(Dir) ->
        filelib:fold_files(Dir, "\\.erl$", true, fun(Path, ok) ->
            Warframe = lists:prefix("apps/wfcore/", Path) orelse
                       string:find(Path, "/games/warframe/") =/= nomatch,
            Prefix = case Warframe of true -> "wf_"; false -> "retinue_" end,
            ?assertEqual({Path, true}, {Path, lists:prefix(Prefix, filename:basename(Path))}),
            ok
        end, ok)
    end, ["apps/retinue_core/src", "apps/wfcore/src", "apps/retinue_cli/src",
          "apps/retinue_daemon/src"]).

platform_owns_cli_test() ->
    {ok, [{application, retinue_cli, Properties}]} =
        file:consult("apps/retinue_cli/src/retinue_cli.app.src"),
    ?assertNot(lists:member(wfcli, proplists:get_value(applications, Properties))),
    ?assertNot(filelib:is_regular("apps/wfcli/src/wfcli.app.src")).

game_dispatch_rejects_unknown_scope_test() ->
    ?assertEqual({error, {unsupported_game, poe2}}, retinue_games:submit(poe2, self(), #{source => player})),
    ?assertEqual({error, {unsupported_game, unknown}}, retinue_games:subscribe(unknown, self(), #{})),
    ?assertEqual(undefined, retinue_games:owner(#{source => worldstate})),
    ?assertEqual(wf_worldstate_service, retinue_games:owner(#{game => warframe, source => worldstate})),
    ?assertEqual(#{observation => not_implemented}, retinue_games:request(arc, status)).

platform_mcp_is_not_warframe_default_test() ->
    {ok, #{<<"tools">> := Tools}} = retinue_mcp_server:request(retinue_mcp, #{<<"method">> => <<"tools/list">>}),
    Names = [maps:get(<<"name">>, Tool) || Tool <- Tools],
    ?assert(lists:member(<<"poe2_session">>, Names)),
    ?assert(lists:member(<<"warframe_query">>, Names)),
    ?assertNot(lists:member(<<"query">>, Names)).

missing_user_bus_is_not_inactive_service_test() ->
    ?assertEqual({ok, true}, retinue_autostart:active_result({0, <<"active\n">>})),
    ?assertEqual({ok, true}, retinue_autostart:active_result({3, <<"activating\n">>})),
    ?assertEqual({ok, false}, retinue_autostart:active_result({3, <<"inactive\n">>})),
    ?assertEqual({ok, false}, retinue_autostart:active_result({4, <<"unknown\n">>})),
    ?assertMatch({error, _}, retinue_autostart:active_result({1, <<"Failed to connect to user bus\n">>})),
    ?assertMatch({error, _}, retinue_autostart:active_result({3, <<"Failed to connect to user bus\n">>})).

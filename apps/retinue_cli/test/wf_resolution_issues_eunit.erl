%%%-------------------------------------------------------------------
%% Persistent current-state registry for metadata resolution failures.
%%%-------------------------------------------------------------------
-module(wf_resolution_issues_eunit).

-include_lib("eunit/include/eunit.hrl").

resolution_issue_registry_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(State) -> fun() -> persists_and_reconciles(State) end end}.

hot_update_discards_obsolete_registry_schema_test() ->
    LegacyIssue = #{<<"scope">> => <<"legacy">>, <<"kind">> => <<"asset">>,
                    <<"identity">> => <<"old">>},
    Legacy = #{schema => 1, path => "/tmp/unused-resolution-issues.json",
               issues => #{{legacy, asset, old} => LegacyIssue},
               subscription => undefined, dirty => false,
               persist_timer => undefined, refresh_error => undefined},
    {ok, Updated} = wf_resolution_issues:code_change(undefined, Legacy, hot_update),
    ?assertEqual(#{}, maps:get(issues, Updated)),
    ?assertEqual(true, maps:get(dirty, Updated)),
    _ = erlang:cancel_timer(maps:get(persist_timer, Updated)),
    receive refresh_current -> ok after 0 -> error(refresh_not_scheduled) end.

setup() ->
    Root = filename:join(
             "/tmp", "wfcli-resolution-" ++
                 integer_to_list(erlang:unique_integer([positive]))),
    Player = filename:join(Root, "player.term"),
    Issues = filename:join(Root, "resolution-issues.json"),
    Catalog = filename:join(Root, "WFCDItems.json"),
    Recipes = filename:join(Root, "ExportRecipes_en.json"),
    ok = filelib:ensure_dir(filename:join(Root, "placeholder")),
    ok = file:write_file(
           Catalog,
           jsone:encode(#{<<"version">> => <<"test">>, <<"fetchedAt">> => 1,
                          <<"entries">> => []})),
    ok = file:write_file(Recipes, jsone:encode(#{<<"ExportRecipes">> => []})),
    application:set_env(retinue_daemon, player_cache, Player),
    application:set_env(retinue_daemon, item_catalog_file, Catalog),
    application:set_env(retinue_daemon, item_recipe_file, Recipes),
    application:set_env(retinue_daemon, resolution_issues_file, Issues),
    {ok, _PlayerPid} = wf_player_service:start_link(),
    {ok, _IssuesPid} = wf_resolution_issues:start_link(),
    #{root => Root, player => Player, issues => Issues, catalog => Catalog,
      recipes => Recipes}.

cleanup(#{root := Root, player := Player, issues := Issues, catalog := Catalog,
          recipes := Recipes}) ->
    stop(wf_resolution_issues),
    stop(wf_player_service),
    application:unset_env(retinue_daemon, player_cache),
    application:unset_env(retinue_daemon, item_catalog_file),
    application:unset_env(retinue_daemon, item_recipe_file),
    application:unset_env(retinue_daemon, resolution_issues_file),
    lists:foreach(fun file:delete/1,
                  [Player ++ ".tmp", Player, Issues ++ ".tmp", Issues, Catalog,
                   Recipes]),
    _ = file:del_dir(Root),
    ok.

persists_and_reconciles(#{issues := Path}) ->
    Scope = <<"test_scope">>,
    Issue = #{<<"kind">> => <<"asset">>, <<"identity">> => <<"/Lotus/Test">>,
              <<"reason">> => <<"missing">>, <<"fallback">> => <<"Test">>,
              <<"private_noise">> => <<"must not persist">>},
    ok = wf_resolution_issues:reconcile(Scope, [Issue]),
    [Current] = wf_resolution_issues:list(),
    ?assertEqual(Scope, maps:get(<<"scope">>, Current)),
    ?assertNot(maps:is_key(<<"private_noise">>, Current)),
    ok = await_file(Path, true, 100),

    ok = gen_server:stop(wf_resolution_issues),
    {ok, _Pid} = wf_resolution_issues:start_link(),
    [Reloaded] = [Value || Value <- wf_resolution_issues:list(),
                           maps:get(<<"scope">>, Value) =:= Scope],
    ?assertEqual(<<"/Lotus/Test">>, maps:get(<<"identity">>, Reloaded)),

    OtherScope = <<"other_scope">>,
    ok = wf_resolution_issues:reconcile(OtherScope, [Issue]),
    ?assertEqual(2, length([Value || Value <- wf_resolution_issues:list(),
                                    maps:get(<<"identity">>, Value) =:=
                                        <<"/Lotus/Test">>])),
    ok = wf_resolution_issues:reconcile(Scope, []),
    [Other] = [Value || Value <- wf_resolution_issues:list(),
                       maps:get(<<"scope">>, Value) =:= OtherScope],
    ?assertEqual(OtherScope, maps:get(<<"scope">>, Other)),
    ok = wf_resolution_issues:reconcile(OtherScope, []),
    ok = await_file(Path, false, 100).

await_file(_Path, _Exists, 0) -> error(persistence_timeout);
await_file(Path, Exists, Attempts) ->
    case filelib:is_file(Path) =:= Exists of
        true -> ok;
        false -> timer:sleep(10), await_file(Path, Exists, Attempts - 1)
    end.

stop(Name) ->
    case whereis(Name) of
        undefined -> ok;
        _Pid -> gen_server:stop(Name)
    end.

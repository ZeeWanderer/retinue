-module(wfcli_asset_resolution_issues_eunit).

-include_lib("eunit/include/eunit.hrl").

asset_failures_are_recorded_and_cleared_test_() ->
    {setup, fun setup/0, fun cleanup/1,
     fun(_Root) -> fun records_and_clears/0 end}.

setup() ->
    Root = filename:join(
             "/tmp", "wfcli-asset-issues-" ++
                 integer_to_list(erlang:unique_integer([positive]))),
    Player = filename:join(Root, "player.term"),
    Issues = filename:join(Root, "resolution-issues.json"),
    Catalog = filename:join(Root, "WFCDItems.json"),
    Recipes = filename:join(Root, "ExportRecipes_en.json"),
    Assets = filename:join(Root, "assets"),
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
    application:set_env(retinue_daemon, asset_cache_dir, Assets),
    application:set_env(retinue_daemon, asset_http_fun,
                        fun(_Url, _Headers) -> {ok, 404, [], <<>>} end),
    {ok, _PlayerPid} = wfcli_player_service:start_link(),
    {ok, _IssuesPid} = wfcli_resolution_issues:start_link(),
    {ok, _AssetPid} = wfcli_asset_service:start_link(),
    Root.

cleanup(Root) ->
    stop(wfcli_asset_service),
    stop(wfcli_resolution_issues),
    stop(wfcli_player_service),
    lists:foreach(
      fun(Key) -> application:unset_env(retinue_daemon, Key) end,
      [player_cache, item_catalog_file, item_recipe_file, resolution_issues_file,
       asset_cache_dir, asset_http_fun]),
    _ = file:del_dir_r(Root),
    ok.

records_and_clears() ->
    Request = [#{<<"id">> => <<"missing">>, <<"source">> => <<"wfcd">>,
                 <<"image_name">> => <<"missing.png">>}],
    {ok, [Failed]} = wfcli_asset_service:resolve(Request),
    ?assertEqual(false, maps:get(<<"ok">>, Failed)),
    [Issue] = [Value || Value <- wfcli_resolution_issues:list(),
                        maps:get(<<"kind">>, Value) =:= <<"asset_fetch">>],
    ?assertEqual(<<"wfcd:missing.png">>, maps:get(<<"identity">>, Issue)),

    application:set_env(
      retinue_daemon, asset_http_fun,
      fun(_Url, _Headers) -> {ok, 200, [], fixture_png()} end),
    {ok, [Resolved]} = wfcli_asset_service:resolve(Request),
    ?assertEqual(true, maps:get(<<"ok">>, Resolved)),
    ?assertEqual([], [Value || Value <- wfcli_resolution_issues:list(),
                              maps:get(<<"kind">>, Value) =:= <<"asset_fetch">>]).

stop(Name) ->
    case whereis(Name) of
        undefined -> ok;
        _Pid -> gen_server:stop(Name)
    end.

fixture_png() ->
    base64:decode(
      <<"iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNk"
        "YAAAAAYAAjCB0C8AAAAASUVORK5CYII=">>).

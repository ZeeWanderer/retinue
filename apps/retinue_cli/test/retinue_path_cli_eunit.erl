%%%-------------------------------------------------------------------
%% EUnit coverage for per-application path reporting.
%%%-------------------------------------------------------------------
-module(retinue_path_cli_eunit).

-include_lib("eunit/include/eunit.hrl").

application_paths_use_shared_xdg_roots_test() ->
    ?assertEqual(
       [{config, wf_paths:config_dir()},
        {data, wf_paths:data_dir()},
        {cache, wf_paths:cache_dir()},
        {state, wf_paths:state_dir()}],
       retinue_path_cli:app_paths(wfcli)),
    ?assert(lists:member(
              {runtime, wf_paths:runtime_dir()},
              retinue_path_cli:app_paths(retinue_daemon))),
    ?assert(lists:member(
              {assets, wf_paths:cache_file("assets")},
              retinue_path_cli:app_paths(retinue_daemon))),
    ?assert(lists:member(
              {data, wf_paths:data_dir()},
              retinue_path_cli:app_paths(retinue_daemon))),
    ?assert(lists:member(
              {state, retinue_paths:root(state)},
              retinue_path_cli:app_paths(retinue_daemon))),
    ?assert(lists:member(
              {cache, retinue_paths:game(cache, poe2)},
              retinue_path_cli:app_paths(retinue_daemon))),
    GuiCache = filename:join(wf_paths:cache_dir(), "gui"),
    ?assertEqual(
       [{config, wf_paths:config_dir()},
        {cache, GuiCache},
        {derivatives, filename:join([GuiCache, "derivatives", "v1"])},
        {runtime, wf_paths:runtime_dir()}],
       retinue_path_cli:app_paths(wfgui)).

aggregate_owners_are_selectable_from_retinue_test() ->
    Owners = [retinue, retinue_daemon, wfcli, wfcompanion, wfgui, poe2companion, arccompanion],
    ?assertEqual(Owners, retinue_path_cli:owners()),
    lists:foreach(fun(Owner) ->
        Args = ["paths", atom_to_list(Owner)],
        ?assertMatch({ok, #{owner := Owner}, ["paths"], _},
                     retinue_cli_args:parse(Args, retinue_cli:command(), "retinue"))
    end, Owners).

platform_report_uses_shared_roots_without_writes_test() ->
    Root = temporary_path("retinue-path-report"),
    Variables = [{"XDG_CONFIG_HOME", "config"}, {"XDG_DATA_HOME", "data"},
                 {"XDG_CACHE_HOME", "cache"}, {"XDG_STATE_HOME", "state"},
                 {"XDG_RUNTIME_DIR", "runtime"}],
    Previous = [{Name, os:getenv(Name)} || {Name, _} <- Variables],
    try
        [os:putenv(Name, filename:join(Root, Dir)) || {Name, Dir} <- Variables],
        Kinds = [config, data, cache, state, runtime],
        Platform = [{Kind, retinue_paths:root(Kind)} || Kind <- Kinds],
        ?assertEqual({retinue, {ok, Platform}}, retinue_path_cli:owner_report(retinue)),
        Daemon = retinue_path_cli:app_paths(retinue_daemon),
        ?assert(lists:member({cache, filename:join([Root, "cache", "retinue", "games", "poe2"])},
                            Daemon)),
        lists:foreach(fun(Kind) ->
            ?assertNot(lists:member({Kind, retinue_paths:game(Kind, poe2)}, Daemon))
        end, [config, data, state]),
        ?assertNot(filelib:is_dir(Root)),
        ?assertEqual(filename:join([Root, "state", "retinue"]),
                     proplists:get_value(state, Platform))
    after
        lists:foreach(fun({Name, false}) -> os:unsetenv(Name);
                         ({Name, Value}) -> os:putenv(Name, Value)
                      end, Previous)
    end.

poe2_paths_come_from_native_report_test() ->
    Json = jsone:encode(#{<<"app">> => <<"poe2companion">>, <<"paths">> => [
        #{<<"kind">> => <<"runtime">>, <<"path">> => <<"/native/socket-parent">>}]}),
    with_poe2_binary(Json, 0, fun() ->
        ?assertEqual({poe2companion, {ok, [{<<"runtime">>, "/native/socket-parent"}]}},
                     retinue_path_cli:owner_report(poe2companion))
    end).

poe2_report_validates_native_owner_test() ->
    Json = jsone:encode(#{<<"app">> => <<"wfcompanion">>, <<"paths">> => []}),
    with_poe2_binary(Json, 0, fun() ->
        ?assertEqual({poe2companion, {error, {unexpected_path_report_app, <<"wfcompanion">>}}},
                     retinue_path_cli:owner_report(poe2companion))
    end).

poe2_native_failures_are_reported_test() ->
    with_poe2_binary(<<"native report failed\n">>, 7, fun() ->
        ?assertEqual({poe2companion, {error, {exit_status, 7, <<"native report failed\n">>}}},
                     retinue_path_cli:owner_report(poe2companion))
    end),
    with_poe2_binary(<<"invalid JSON">>, 0, fun() ->
        ?assertMatch({poe2companion, {error, {invalid_path_report_json, _}}},
                     retinue_path_cli:owner_report(poe2companion))
    end).

owner_json_decodes_to_unformatted_paths_test() ->
    Json = jsone:encode(
             #{<<"app">> => <<"wfcompanion">>,
               <<"paths">> => [#{<<"kind">> => <<"cache">>,
                                  <<"path">> => <<"/tmp/retinue/games/warframe">>}]}),
    ?assertEqual(
       {ok, [{<<"cache">>, "/tmp/retinue/games/warframe"}]},
       retinue_path_cli:decode_owner_report(<<"wfcompanion">>, Json)).

tree_renderer_handles_nested_nodes_test() ->
    ?assertEqual(
       <<"root\n├─ first\n└─ second\n   └─ leaf\n"/utf8>>,
       iolist_to_binary(
         retinue_tree:format(
           {<<"root">>, [{<<"first">>, []},
                          {<<"second">>, [{<<"leaf">>, []}]}]}))).

tree_renderer_handles_more_than_ten_labels_test() ->
    Children = [{["node-", integer_to_list(Index)], []}
                || Index <- lists:seq(1, 12)],
    Output = iolist_to_binary(retinue_tree:format({<<"root">>, Children})),
    ?assertMatch({_, _}, binary:match(Output, <<"node-10\n">>)),
    ?assertMatch({_, _}, binary:match(Output, <<"node-12\n">>)).

merged_tree_nests_logical_children_below_symlink_test() ->
    Link = "/home/test/.cache/retinue",
    Target = "/mnt/cache/retinue",
    Warframe = filename:join([Link, "games", "warframe"]),
    Descriptions =
        [#{path => Link, status => directory, links => [{Link, Target}]},
         #{path => filename:join(Warframe, "captures"), status => directory,
           links => [{Link, Target}]},
         #{path => filename:join([Warframe, "gui", "derivatives", "v1"]),
           status => missing, links => [{Link, Target}]}],
    ?assertEqual(
       <<"/\n"
         "└─ home\n"
         "   └─ test\n"
         "      └─ .cache\n"
         "         └─ retinue -> /mnt/cache/retinue\n"
         "            └─ games\n"
         "               └─ warframe\n"
         "                  ├─ captures\n"
         "                  └─ gui\n"
         "                     └─ derivatives\n"
         "                        └─ v1 (missing)\n"/utf8>>,
       iolist_to_binary(
         retinue_tree:format(retinue_path_cli:filesystem_tree(Descriptions)))).

symlink_target_is_reported_test() ->
    Base = filename:join(
             "/tmp",
             "wfcli-path-report-" ++
             integer_to_list(erlang:unique_integer([positive]))),
    Link = Base ++ "-link",
    ok = file:make_dir(Base),
    ok = file:make_symlink(filename:basename(Base), Link),
    try
        ?assertEqual({directory, [{Link, Base}]}, retinue_path_cli:describe(Link))
    after
        file:delete(Link),
        file:del_dir(Base)
    end.

linked_parent_is_resolved_through_missing_tail_test() ->
    Root = temporary_path("wfcli-path-parent"),
    Real = filename:join(Root, "real"),
    Link = filename:join(Root, "linked"),
    Logical = filename:join([Link, "nested", "cache"]),
    ok = file:make_dir(Root),
    ok = file:make_dir(Real),
    ok = file:make_symlink("real", Link),
    try
        ?assertEqual({missing, [{Link, Real}]}, retinue_path_cli:describe(Logical))
    after
        file:delete(Link),
        file:del_dir(Real),
        file:del_dir(Root)
    end.

missing_path_without_link_stays_missing_test() ->
    ?assertEqual({missing, []},
                 retinue_path_cli:describe(temporary_path("wfcli-path-missing"))).

system_ancestors_are_hidden_but_user_chain_is_kept_test() ->
    Home = "/home/tester",
    ?assertEqual(
       [{"/home/tester/.cache/retinue", "/mnt/cache"},
        {"/mnt/cache", "/data/cache"}],
       retinue_path_cli:links_in_scope(
         "/home/tester/.cache/retinue/games/warframe/data",
         [{"/home", "/var/home"},
          {"/home/tester/.cache/retinue", "/mnt/cache"},
          {"/mnt/cache", "/data/cache"}],
         Home)).

temporary_path(Prefix) ->
    filename:join(
      "/tmp",
      Prefix ++ "-" ++ os:getpid() ++ "-" ++ integer_to_list(erlang:unique_integer([positive]))).

with_poe2_binary(Output, Status, Run) ->
    Root = temporary_path("retinue-native-paths"),
    Executable = filename:join(Root, "poe2 companion"),
    Previous = os:getenv("POE2COMPANION_BIN"),
    ok = file:make_dir(Root),
    try
        ok = file:write_file(Executable ++ ".json", Output),
        ok = file:write_file(Executable, [
            "#!/bin/sh\n",
            "[ \"$#\" -eq 2 ] && [ \"$1\" = paths ] && [ \"$2\" = --json ] || exit 64\n",
            "/bin/cat \"$0.json\"\nexit ", integer_to_list(Status), "\n"]),
        ok = file:change_mode(Executable, 8#700),
        true = os:putenv("POE2COMPANION_BIN", Executable),
        Run()
    after
        case Previous of
            false -> os:unsetenv("POE2COMPANION_BIN");
            _ -> os:putenv("POE2COMPANION_BIN", Previous)
        end,
        file:del_dir_r(Root)
    end.

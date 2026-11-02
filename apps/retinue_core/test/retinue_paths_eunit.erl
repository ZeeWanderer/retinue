-module(retinue_paths_eunit).
-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

xdg_namespaces_test() ->
    with_env([{"HOME", "/test/home"}, {"XDG_CACHE_HOME", "/store/cache"},
              {"XDG_CONFIG_HOME", "relative"}, {"XDG_DATA_HOME", ""},
              {"XDG_STATE_HOME", "/store/state"}, {"XDG_RUNTIME_DIR", "/run/test"}], fun() ->
        ?assertEqual("/store/cache/retinue/games/warframe", wf_paths:cache_dir()),
        ?assertEqual("/store/cache/retinue/games/poe2", retinue_paths:game(cache, poe2)),
        ?assertEqual("/test/home/.config/retinue/games/warframe", wf_paths:config_dir()),
        ?assertEqual("/test/home/.local/share/retinue/games/warframe/builds.term",
                     wf_paths:data_file("builds.term")),
        ?assertEqual("/store/state/retinue/daemon.log", retinue_incidents:path(daemon)),
        ?assertEqual("/run/test/retinue/daemon.sock", retinue_paths:file(runtime, "daemon.sock"))
    end).

runtime_fallback_is_local_test() ->
    with_env([{"XDG_RUNTIME_DIR", "relative"}, {"XDG_CACHE_HOME", "/mounted/cache"}], fun() ->
        ?assertEqual("/tmp/retinue-" ++ integer_to_list(retinue_paths:uid()),
                     retinue_paths:root(runtime))
    end).

runtime_rejects_symlink_and_public_directory_test() ->
    Root = temporary(),
    ok = file:make_dir(Root),
    ok = file:change_mode(Root, 8#700),
    with_env([{"XDG_RUNTIME_DIR", Root}], fun() ->
        Path = retinue_paths:root(runtime),
        try
            ?assertEqual(ok, retinue_paths:ensure_runtime()),
            {ok, #file_info{mode = Mode}} = file:read_file_info(Path),
            ?assertEqual(8#700, Mode band 8#777),
            ok = file:change_mode(Path, 8#755),
            ?assertMatch({error, {unsafe_runtime_directory, _}}, retinue_paths:ensure_runtime()),
            ok = file:del_dir(Path),
            ok = file:make_symlink(Root, Path),
            ?assertMatch({error, {unsafe_runtime_directory, _}}, retinue_paths:ensure_runtime())
        after
            file:delete(Path), file:del_dir(Path), file:del_dir(Root)
        end
    end).

persistent_roots_allow_symlinks_test() ->
    Root = temporary(),
    Target = Root ++ "-backing",
    ok = file:make_dir(Target),
    ok = file:make_symlink(Target, Root),
    try
        ?assertEqual(ok, retinue_paths:ensure_private(filename:join(Root, "game"))),
        ?assert(filelib:is_dir(filename:join(Target, "game"))),
        ?assertMatch({ok, _}, file:read_link(Root))
    after
        file:del_dir(filename:join(Target, "game")), file:delete(Root), file:del_dir(Target)
    end.

temporary() -> "/tmp/retinue-paths-" ++ integer_to_list(erlang:unique_integer([positive, monotonic])).

with_env(Values, Fun) ->
    Previous = [{Name, os:getenv(Name)} || {Name, _} <- Values],
    lists:foreach(fun({Name, Value}) -> os:putenv(Name, Value) end, Values),
    try Fun()
    after lists:foreach(fun
        ({Name, false}) -> os:unsetenv(Name);
        ({Name, Value}) -> os:putenv(Name, Value)
    end, Previous) end.

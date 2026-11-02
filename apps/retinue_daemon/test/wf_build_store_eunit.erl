%%%-------------------------------------------------------------------
%% EUnit coverage for the durable build repository.
%%%-------------------------------------------------------------------
-module(wf_build_store_eunit).

-include_lib("eunit/include/eunit.hrl").
-include_lib("kernel/include/file.hrl").

round_trip_test_() ->
    {setup,
     fun() ->
         Root = filename:join("/tmp", "wfcli-build-store-" ++
                             integer_to_list(erlang:unique_integer([positive]))),
         Path = filename:join(Root, "builds.term"),
         application:set_env(retinue_daemon, build_store_file, Path),
         application:set_env(retinue_daemon, build_cache_file, Path ++ ".cache"),
         #{root => Root, path => Path}
     end,
     fun(#{root := Root}) ->
         application:unset_env(retinue_daemon, build_store_file),
         application:unset_env(retinue_daemon, build_cache_file),
         _ = file:del_dir_r(Root)
     end,
     fun(#{path := Path}) -> fun() ->
         {ok, Empty} = wf_build_store:load(),
         Store = Empty#{goals => #{<<"goal">> => #{item => <<"test">>}}},
         ok = wf_build_store:save(Store),
         Cache = Store#{revisions => #{cached => value}},
         ok = wf_build_store:save_cache(Cache),
         {ok, Loaded} = wf_build_store:load(),
         ?assertEqual(maps:get(goals, Store), maps:get(goals, Loaded)),
         ?assertEqual(#{cached => value}, maps:get(revisions, Loaded)),
         {ok, Raw} = file:read_file(Path),
         ?assertEqual([goals, schema], lists:sort(maps:keys(binary_to_term(Raw)))),
         ok = file:write_file(wf_build_store:cache_path(), <<"invalid cache">>),
         {ok, WithoutCache} = wf_build_store:load(),
         ?assertEqual(maps:get(goals, Store), maps:get(goals, WithoutCache)),
         ?assertEqual(#{}, maps:get(revisions, WithoutCache)),
         {ok, Info} = file:read_file_info(Path),
         ?assertEqual(8#600, Info#file_info.mode band 8#777)
     end end}.

migrates_schema_one_store_test_() ->
    {setup,
     fun() ->
         Root = filename:join("/tmp", "wfcli-build-store-migrate-" ++
                             integer_to_list(erlang:unique_integer([positive]))),
         Path = filename:join(Root, "builds.term"),
         ok = filelib:ensure_dir(Path),
         application:set_env(retinue_daemon, build_store_file, Path),
         application:set_env(retinue_daemon, build_cache_file, Path ++ ".cache"),
         Legacy = #{schema => 1, catalogs => #{}, revisions => #{}, latest => #{},
                    goals => #{<<"group">> => #{<<"name">> => <<"Kept">>}},
                    results => #{}},
         ok = file:write_file(Path, term_to_binary(Legacy, [deterministic])),
         #{root => Root}
     end,
     fun(#{root := Root}) ->
         application:unset_env(retinue_daemon, build_store_file),
         application:unset_env(retinue_daemon, build_cache_file),
         _ = file:del_dir_r(Root)
     end,
     fun(_State) -> fun() ->
         {ok, Migrated} = wf_build_store:load(),
         ?assertEqual(3, maps:get(schema, Migrated)),
         ?assertMatch(#{<<"group">> := #{<<"name">> := <<"Kept">>}},
                      maps:get(goals, Migrated))
     end end}.

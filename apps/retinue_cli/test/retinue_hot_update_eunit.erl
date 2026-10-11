%%%-------------------------------------------------------------------
%% EUnit tests for coordinated daemon hot loading.
%%%-------------------------------------------------------------------
-module(retinue_hot_update_eunit).

-include_lib("eunit/include/eunit.hrl").

read_build_directory_test() ->
    Dir = filename:join(code:lib_dir(retinue_daemon), "ebin"),
    {ok, Bundles} = retinue_hot_update:read_directory(Dir),
    ?assert(lists:any(
              fun(#{module := Module}) -> Module =:= retinue_hot_update end,
              Bundles)).

retired_cli_module_is_not_loadable_test() ->
    lists:foreach(fun(Module) ->
        {ok, Module, Binary} = compile_fixture(Module, 1),
        Bundle = #{module => Module, filename => atom_to_list(Module) ++ ".beam", binary => Binary},
        ?assertEqual({error, {module_not_allowed, Module}}, retinue_hot_update:apply([Bundle]))
    end, [wfcli, wfcli_retired_fixture]).

read_embedded_daemon_applications_test() ->
    {ok, Bundles} = retinue_hot_update:read_applications([retinue_core, wfcore, retinue_daemon]),
    Modules = [Module || #{module := Module, binary := Binary} <- Bundles,
                         is_binary(Binary)],
    ?assert(lists:member(wf_protocol, Modules)),
    ?assert(lists:member(retinue_hot_update, Modules)),
    ?assertEqual(length(Modules), length(lists:usort(Modules))).

build_identity_is_order_independent_and_tracks_modules_test() ->
    {ok, One, BeamOne} = compile_fixture(retinue_hot_update_fixture_one, 1),
    {ok, Two, BeamTwo} = compile_fixture(retinue_hot_update_fixture_two, 2),
    BundleOne = #{module => One, filename => "one.beam", binary => BeamOne},
    BundleTwo = #{module => Two, filename => "two.beam", binary => BeamTwo},
    {ok, Both} = retinue_hot_update:build_identity([BundleOne, BundleTwo]),
    {ok, Reversed} = retinue_hot_update:build_identity([BundleTwo, BundleOne]),
    {ok, OnlyOne} = retinue_hot_update:build_identity([BundleOne]),
    ?assertEqual(Both, Reversed),
    ?assertNotEqual(Both, OnlyOne),
    ?assertEqual(64, byte_size(Both)).

loads_changed_beam_and_skips_identical_beam_test() ->
    Module = retinue_hot_update_fixture,
    cleanup_module(Module),
    {ok, Module, V1} = compile_fixture(Module, 1),
    {module, Module} = code:load_binary(Module, "retinue_hot_update_fixture.beam", V1),
    try
        ?assertEqual(1, Module:value()),
        {ok, Module, V2} = compile_fixture(Module, 2),
        Bundle = #{module => Module,
                   filename => "retinue_hot_update_fixture.beam",
                   binary => V2},
        {ok, Changed} = retinue_hot_update:apply([Bundle]),
        ?assertEqual([Module], maps:get(loaded, Changed)),
        ?assertEqual(2, Module:value()),
        {ok, Unchanged} = retinue_hot_update:apply([Bundle]),
        ?assertEqual([], maps:get(loaded, Unchanged)),
        ?assertEqual([Module], maps:get(unchanged, Unchanged))
    after
        cleanup_module(Module)
    end.

prepare_failure_does_not_partially_load_bundle_test() ->
    Module = retinue_hot_update_atomic_fixture,
    OnLoadModule = retinue_hot_update_on_load_fixture,
    cleanup_module(Module),
    cleanup_module(OnLoadModule),
    {ok, Module, V1} = compile_fixture(Module, 1),
    {module, Module} = code:load_binary(Module, "atomic_fixture.beam", V1),
    try
        {ok, Module, V2} = compile_fixture(Module, 2),
        {ok, OnLoadModule, OnLoad} = compile_on_load_fixture(OnLoadModule),
        Bundles = [#{module => Module, filename => "atomic_fixture.beam", binary => V2},
                   #{module => OnLoadModule, filename => "on_load_fixture.beam",
                     binary => OnLoad}],
        ?assertMatch({error, {prepare_failed, _}}, retinue_hot_update:apply(Bundles)),
        ?assertEqual(1, Module:value())
    after
        cleanup_module(Module),
        cleanup_module(OnLoadModule)
    end.

stateful_service_runs_code_change_test() ->
    Module = wf_worldstate_service,
    ensure_service_stopped(),
    {Module, Original, Filename} = code:get_object_code(Module),
    {ok, Module, Changed} = recompile_with_test_vsn(Module, Original),
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    {ok, Pid} = Module:start_link(),
    try
        Bundle = #{module => Module,
                   filename => "wf_worldstate_service.beam",
                   binary => Changed},
        {ok, Result} = retinue_hot_update:apply([Bundle]),
        ?assertEqual([Module], maps:get(loaded, Result)),
        ?assertEqual([Module], maps:get(migrated, Result)),
        ?assert(is_process_alive(Pid)),
        ?assert(is_map(Module:status()))
    after
        gen_server:stop(Pid),
        restore_module(Module, Filename, Original)
    end.

all_supervised_stateful_services_are_migrated_test() ->
    Children = retinue_sup:worker_specs(),
    ?assertEqual(
       lists:sort([Module || #{modules := Modules} <- Children, Module <- Modules]),
       lists:sort([Module || {_Name, Module} <- retinue_hot_update:stateful_candidates()])),
    lists:foreach(fun({_, Module}) ->
                      {module, Module} = code:ensure_loaded(Module),
                      ?assert(erlang:function_exported(Module, code_change, 3))
                  end, retinue_hot_update:stateful_candidates()).

local_api_can_be_restarted_for_code_purge_test() ->
    Started = whereis(retinue_sup) =:= undefined,
    case Started of true -> ok = retinue_test_daemon:start(); false -> ok end,
    try
        Old = whereis(retinue_local_api),
        OldDaemon = whereis(retinue_daemon),
        Activity = whereis(retinue_activity),
        Games = whereis(retinue_games_sup),
        ?assert(is_pid(Old)),
        ok = retinue_hot_update:restart_supervised_child(retinue_local_api),
        New = whereis(retinue_local_api),
        ?assert(is_pid(New)),
        ?assertNotEqual(Old, New),
        ?assert(is_process_alive(New)),
        ?assertEqual(OldDaemon, whereis(retinue_daemon)),
        ?assert(is_process_alive(OldDaemon)),
        ?assertEqual(Activity, whereis(retinue_activity)),
        ?assertEqual(Games, whereis(retinue_games_sup))
    after
        case Started of true -> retinue_test_daemon:stop(); false -> ok end
    end.

manual_socket_hot_update_replies_through_daemon_test_() ->
    [{atom_to_list(Module), {timeout, 15, fun() ->
        manual_socket_hot_update_replies_through_daemon(Module)
    end}} || Module <- [retinue_local_api, retinue_local_protocol]].

manual_socket_hot_update_replies_through_daemon(Module) ->
    {Module, Original, Filename} = code:get_object_code(Module),
    {ok, Module, Changed} = recompile_with_test_vsn(Module, Original),
    ok = retinue_test_daemon:start(),
    try
        {ok, Registered} = application:get_key(retinue_daemon, registered),
        Before = [{Name, whereis(Name)} || Name <- Registered],
        ok = retinue_activity:activity_start(),
        Bundle = #{module => Module, filename => Filename, binary => Changed},
        ?assertMatch({ok, #{loaded := [Module]}},
                     retinue_daemon:request({hot_update, [Bundle]})),
        lists:foreach(fun
            ({retinue_local_api, Pid}) ->
                ?assert(is_pid(whereis(retinue_local_api))),
                ?assertNotEqual(Pid, whereis(retinue_local_api)),
                ?assertNot(is_process_alive(Pid));
            ({Name, Pid}) -> ?assertEqual({Name, Pid}, {Name, whereis(Name)})
        end, Before),
        ?assertMatch(#{external_activity := 1}, retinue_activity:status()),
        ?assertMatch(#{status := running, update := idle}, retinue_daemon:request(status)),
        ok = retinue_activity:activity_end()
    after
        retinue_test_daemon:stop(),
        persistent_term:erase({retinue_hot_update, pending_migrations}),
        restore_module(Module, Filename, Original)
    end.

local_protocol_changes_restart_native_connections_test() ->
    ?assert(retinue_hot_update:runtime_restart_required([retinue_local_protocol])),
    ?assert(retinue_hot_update:runtime_restart_required([retinue_local_api])),
    ?assertNot(retinue_hot_update:runtime_restart_required([wf_protocol])).

failed_migration_restarts_from_durable_state_test() ->
    Module = wf_build_service,
    {Module, Original, Filename} = code:get_object_code(Module),
    {ok, Module, Changed} = failing_migration(Module, Original),
    ok = retinue_test_daemon:start(),
    try
        {ok, Group} = Module:create_group(
                        #{<<"definition_id">> => <<"/test">>},
                        #{<<"definitions">> => [#{<<"id">> => <<"/test">>,
                                                   <<"name">> => <<"Test">>}]}),
        Id = maps:get(<<"id">>, Group),
        Pid = whereis(Module),
        Bundle = #{module => Module, filename => Filename, binary => Changed},
        ?assertMatch({error, {code_change_failed, Module, _}},
                     retinue_hot_update:apply([Bundle])),
        NewPid = whereis(Module),
        ?assert(is_pid(NewPid)),
        ?assertNotEqual(Pid, NewPid),
        ?assertMatch({ok, #{<<"id">> := Id}}, Module:group(Id)),
        ?assertMatch({ok, #{loaded := [], migrated := []}},
                     retinue_hot_update:apply([Bundle])),
        ?assertEqual(NewPid, whereis(Module))
    after
        retinue_test_daemon:stop(),
        persistent_term:erase({retinue_hot_update, pending_migrations}),
        restore_module(Module, Filename, Original)
    end.

failed_activity_migration_resets_held_lifetimes_test_() ->
    {timeout, 15, fun() ->
        Module = retinue_activity,
        {Module, Original, Filename} = code:get_object_code(Module),
        {ok, Module, Changed} = failing_migration(Module, Original),
        SavedFetch = application:get_env(retinue_daemon, daemon_worldstate_fetch_fun),
        application:set_env(retinue_daemon, daemon_worldstate_fetch_fun,
                            fun() -> {error, test_offline} end),
        ok = retinue_test_daemon:start(),
        try
            {ok, PlayerCache} = application:get_env(retinue_daemon, player_cache),
            Cache = filename:join(filename:dirname(PlayerCache), "migration-worldstate.json"),
            Request = #{source => worldstate, mode => list, type_filter => alert,
                        specs => [#{label => "alerts", type_filter => alert}],
                        opts => #{cache => Cache, refresh => true}},
            {ok, _Watch} = wf_worldstate_service:subscribe(self(), Request),
            ?assertMatch(#{external_activity := 1}, Module:status()),
            {ok, Registered} = application:get_key(retinue_daemon, registered),
            Before = [{Name, whereis(Name)} || Name <- Registered],
            Bundle = #{module => Module, filename => Filename, binary => Changed},
            ?assertMatch({error, {code_change_failed, Module, _}},
                         retinue_hot_update:apply([Bundle])),
            lists:foreach(fun
                ({retinue_sup, Pid}) -> ?assertEqual(Pid, whereis(retinue_sup));
                ({Name, Pid}) ->
                    ?assert(is_pid(whereis(Name))),
                    ?assertNotEqual({Name, Pid}, {Name, whereis(Name)}),
                    ?assertNot(is_process_alive(Pid))
            end, Before),
            ?assertMatch(#{watches := 0}, wf_worldstate_service:status()),
            ?assertMatch(#{external_activity := 0}, Module:status()),
            ?assertMatch({ok, #{loaded := [], migrated := []}},
                         retinue_hot_update:apply([Bundle]))
        after
            retinue_test_daemon:stop(),
            persistent_term:erase({retinue_hot_update, pending_migrations}),
            restore_module(Module, Filename, Original),
            case SavedFetch of
                undefined -> application:unset_env(retinue_daemon, daemon_worldstate_fetch_fun);
                {ok, Fetch} -> application:set_env(retinue_daemon, daemon_worldstate_fetch_fun, Fetch)
            end
        end
    end}.

unrecovered_migration_never_succeeds_on_retry_test() ->
    Module = wf_worldstate_service,
    ensure_service_stopped(),
    {Module, Original, Filename} = code:get_object_code(Module),
    {ok, Module, Changed} = failing_migration(Module, Original),
    application:set_env(retinue_daemon, daemon_idle_shutdown, false),
    {ok, Pid} = Module:start_link(),
    try
        Bundle = #{module => Module, filename => Filename, binary => Changed},
        ?assertMatch({error, {migration_recovery_failed, _, _}},
                     retinue_hot_update:apply([Bundle])),
        ?assertEqual({error, {migration_requires_restart, Module}},
                      retinue_hot_update:apply([Bundle]))
    after
        gen_server:stop(Pid),
        persistent_term:erase({retinue_hot_update, pending_migrations}),
        restore_module(Module, Filename, Original)
    end.

failing_migration(Module, Binary) ->
    {ok, {Module, [{abstract_code, {raw_abstract_v1, Forms}}]}} =
        beam_lib:chunks(Binary, [abstract_code]),
    Changed = [case Form of
        {function, Line, code_change, 3, _} ->
            {function, Line, code_change, 3,
             [{clause, Line, [{var, Line, '_'}, {var, Line, '_'}, {var, Line, '_'}],
               [], [erl_parse:abstract({error, forced_migration_failure})]}]};
        _ -> Form
    end || Form <- Forms],
    compile:forms(Changed, [binary, debug_info]).

compile_fixture(Module, Value) ->
    Forms = [
        {attribute, 1, module, Module},
        {attribute, 2, export, [{value, 0}]},
        {function, 3, value, 0,
         [{clause, 3, [], [], [{integer, 3, Value}]}]}
    ],
    case compile:forms(Forms, [binary]) of
        {ok, Module, Binary} -> {ok, Module, Binary};
        {ok, Module, Binary, _Warnings} -> {ok, Module, Binary}
    end.

compile_on_load_fixture(Module) ->
    Forms = [
        {attribute, 1, module, Module},
        {attribute, 2, on_load, {init, 0}},
        {attribute, 3, export, [{init, 0}]},
        {function, 4, init, 0,
         [{clause, 4, [], [], [{atom, 4, ok}]}]}
    ],
    case compile:forms(Forms, [binary]) of
        {ok, Module, Binary} -> {ok, Module, Binary};
        {ok, Module, Binary, _Warnings} -> {ok, Module, Binary}
    end.

recompile_with_test_vsn(Module, Binary) ->
    {ok, {Module, [{abstract_code, {raw_abstract_v1, Forms}}]}} =
        beam_lib:chunks(Binary, [abstract_code]),
    WithoutVsn = [Form || Form <- Forms, not is_vsn_attribute(Form)],
    WithVsn = insert_vsn(WithoutVsn, []),
    case compile:forms(WithVsn, [binary, debug_info]) of
        {ok, Module, Changed} -> {ok, Module, Changed};
        {ok, Module, Changed, _Warnings} -> {ok, Module, Changed}
    end.

is_vsn_attribute({attribute, _Line, vsn, _Value}) -> true;
is_vsn_attribute(_Form) -> false.

insert_vsn([{attribute, Line, module, _Module} = Form | Rest], Acc) ->
    lists:reverse(Acc) ++ [Form, {attribute, Line, vsn, hot_update_test} | Rest];
insert_vsn([Form | Rest], Acc) ->
    insert_vsn(Rest, [Form | Acc]).

restore_module(Module, Filename, Original) ->
    true = code:soft_purge(Module),
    {module, Module} = code:load_binary(Module, Filename, Original),
    _ = code:purge(Module),
    ok.

ensure_service_stopped() ->
    case whereis(wf_worldstate_service) of
        undefined -> ok;
        _Pid -> gen_server:stop(wf_worldstate_service)
    end.

cleanup_module(Module) ->
    _ = code:purge(Module),
    _ = code:delete(Module),
    _ = code:purge(Module),
    ok.

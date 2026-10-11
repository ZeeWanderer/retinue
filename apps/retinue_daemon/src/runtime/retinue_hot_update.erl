%%%-------------------------------------------------------------------
%% Coordinated local BEAM hot loading for the persistent daemon.
%%%-------------------------------------------------------------------
-module(retinue_hot_update).

-export([read_directory/1, read_directories/1, read_applications/1,
         build_identity/1, current_build_identity/0, apply/1]).

-ifdef(TEST).
-export([stateful_candidates/0, restart_supervised_child/1,
         runtime_restart_required/1]).
-endif.

-define(SYS_TIMEOUT_MS, 5000).
-define(BUILD_ID_KEY, {?MODULE, build_identity}).
-define(PENDING_KEY, {?MODULE, pending_migrations}).

-type bundle() :: #{module := module(), filename := string(), binary := binary()}.
-type build_identity() :: binary().
-type result() :: #{loaded := [module()], unchanged := [module()], migrated := [module()]}.

-doc "Read and validate every Retinue BEAM in one build ebin directory.".
-spec read_directory(file:filename_all()) -> {ok, [bundle()]} | {error, term()}.
read_directory(Dir) ->
    case file:list_dir(Dir) of
        {ok, Entries} ->
            BeamFiles = lists:sort(
                          [filename:join(Dir, Entry) || Entry <- Entries,
                                                       filename:extension(Entry) =:= ".beam"]),
            read_beams(BeamFiles, []);
        {error, Reason} ->
            {error, {beam_directory_unreadable, Dir, Reason}}
    end.

-doc "Read validated BEAM bundles from multiple umbrella application directories.".
-spec read_directories([file:filename_all()]) -> {ok, [bundle()]} | {error, term()}.
read_directories(Dirs) ->
    read_directories(Dirs, []).

read_directories([], Acc) -> {ok, lists:append(lists:reverse(Acc))};
read_directories([Dir | Rest], Acc) ->
    case read_directory(Dir) of
        {ok, Bundles} -> read_directories(Rest, [Bundles | Acc]);
        {error, Reason} -> {error, Reason}
    end.

-doc "Read validated BEAM bundles embedded in loaded OTP applications.".
-spec read_applications([atom()]) -> {ok, [bundle()]} | {error, term()}.
read_applications(Apps) ->
    read_applications(Apps, []).

read_applications([], Acc) ->
    {ok, lists:append(lists:reverse(Acc))};
read_applications([App | Rest], Acc) ->
    case application:load(App) of
        ok -> read_application_modules(App, Rest, Acc);
        {error, {already_loaded, App}} -> read_application_modules(App, Rest, Acc);
        {error, Reason} -> {error, {application_load_failed, App, Reason}}
    end.

read_application_modules(App, Rest, Acc) ->
    case application:get_key(App, modules) of
        {ok, Modules} ->
            case read_modules(Modules, []) of
                {ok, Bundles} -> read_applications(Rest, [Bundles | Acc]);
                {error, Reason} -> {error, {application_beams_unavailable, App, Reason}}
            end;
        undefined ->
            {error, {application_modules_missing, App}}
    end.

-doc "Return a deterministic identity for one complete daemon/shared BEAM bundle set.".
-spec build_identity([bundle()]) -> {ok, build_identity()} | {error, term()}.
build_identity(Bundles) when is_list(Bundles) ->
    case bundle_manifest(Bundles, []) of
        {ok, Manifest} ->
            Hash = crypto:hash(sha256, term_to_binary(lists:sort(Manifest), [deterministic])),
            {ok, binary:encode_hex(Hash, lowercase)};
        {error, _Reason} = Error -> Error
    end;
build_identity(Other) ->
    {error, {invalid_hot_update, Other}}.

-doc "Return identity of code installed at daemon startup or by the latest hot update.".
-spec current_build_identity() -> {ok, build_identity()} | {error, term()}.
current_build_identity() ->
    case persistent_term:get(?BUILD_ID_KEY, undefined) of
        undefined ->
            case read_applications([retinue_core, wfcore, retinue_daemon]) of
                {ok, Bundles} ->
                    case build_identity(Bundles) of
                        {ok, Identity} = Result ->
                            persistent_term:put(?BUILD_ID_KEY, Identity),
                            Result;
                        {error, _Reason} = Error -> Error
                    end;
                {error, _ReadReason} = Error -> Error
            end;
        Identity when is_binary(Identity) ->
            {ok, Identity}
    end.

-doc "Load changed wfcli modules and run code_change/3 for daemon-owned gen_servers.".
-spec apply([bundle()]) -> {ok, result()} | {error, term()}.
apply(Bundles) when is_list(Bundles) ->
    case validate_bundles(Bundles) of
        ok ->
            case build_identity(Bundles) of
                {ok, BuildIdentity} -> apply_validated(Bundles, BuildIdentity);
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end;
apply(Other) ->
    {error, {invalid_hot_update, Other}}.

read_beams([], Acc) ->
    {ok, lists:reverse(Acc)};
read_beams([Path | Rest], Acc) ->
    case file:read_file(Path) of
        {ok, Binary} ->
            case beam_module(Binary) of
                {ok, Module} ->
                    Bundle = #{module => Module,
                               filename => filename:basename(Path),
                               binary => Binary},
                    read_beams(Rest, [Bundle | Acc]);
                {error, Reason} ->
                    {error, {invalid_beam, Path, Reason}}
            end;
        {error, Reason} ->
            {error, {beam_read_failed, Path, Reason}}
    end.

read_modules([], Acc) ->
    {ok, lists:reverse(Acc)};
read_modules([Module | Rest], Acc) ->
    case code:get_object_code(Module) of
        {Module, Binary, Filename} ->
            Bundle = #{module => Module,
                       filename => filename:basename(Filename),
                       binary => Binary},
            read_modules(Rest, [Bundle | Acc]);
        error ->
            {error, {object_code_missing, Module}}
    end.

validate_bundles(Bundles) ->
    case validate_bundles(Bundles, []) of
        {ok, Modules} ->
            case length(Modules) =:= length(lists:usort(Modules)) of
                true -> ok;
                false -> {error, duplicate_modules}
            end;
        Error -> Error
    end.

validate_bundles([], Modules) ->
    {ok, Modules};
validate_bundles([#{module := Module, filename := Filename, binary := Binary} | Rest], Modules)
  when is_atom(Module), is_list(Filename), is_binary(Binary) ->
    case {allowed_module(Module), beam_module(Binary)} of
        {true, {ok, Module}} -> validate_bundles(Rest, [Module | Modules]);
        {false, _} -> {error, {module_not_allowed, Module}};
        {true, {ok, Other}} -> {error, {beam_module_mismatch, Module, Other}};
        {true, {error, Reason}} -> {error, {invalid_beam, Filename, Reason}}
    end;
validate_bundles([Bundle | _Rest], _Modules) ->
    {error, {invalid_bundle, Bundle}}.

bundle_manifest([], Acc) ->
    {ok, Acc};
bundle_manifest([#{module := Module, binary := Binary} | Rest], Acc)
  when is_atom(Module), is_binary(Binary) ->
    case beam_md5(Binary) of
        {ok, Module, Md5} -> bundle_manifest(Rest, [{Module, Md5} | Acc]);
        {ok, Other, _Md5} -> {error, {beam_module_mismatch, Module, Other}};
        {error, Reason} -> {error, {invalid_beam, Module, Reason}}
    end;
bundle_manifest([Bundle | _Rest], _Acc) ->
    {error, {invalid_bundle, Bundle}}.

allowed_module(Module) ->
    Name = atom_to_list(Module),
    lists:prefix("retinue_", Name) orelse lists:prefix("wf_", Name).

apply_validated(Bundles, BuildIdentity) ->
    case recover_pending() of
        ok -> apply_recovered(Bundles, BuildIdentity);
        {error, _} = Error -> Error
    end.

apply_recovered(Bundles, BuildIdentity) ->
    {Changed0, Unchanged} = lists:partition(fun bundle_changed/1, Bundles),
    Changed = Changed0,
    Result = case Changed of
        [] ->
            reconcile_unchanged(Unchanged);
        _ ->
            case purge_previous_versions(Changed) of
                ok -> apply_changed(Changed, Unchanged);
                {error, _Reason} = Error -> Error
            end
    end,
    case Result of
        {ok, _} ->
            maybe_store_build_identity(Bundles, BuildIdentity),
            Result;
        {error, _ResultReason} ->
            Result
    end.

reconcile_unchanged(Unchanged) ->
    case ensure_supervised_children() of
        ok ->
            Result = #{loaded => [], unchanged => module_names(Unchanged), migrated => []},
            case restart_supervised_child(retinue_local_api) of
                ok -> {ok, Result};
                {error, Reason} ->
                    {error, {runtime_restart_failed, retinue_local_api, Reason}}
            end;
        {error, Reason} ->
            {error, {supervisor_reconcile_failed, Reason}}
    end.

maybe_store_build_identity(Bundles, BuildIdentity) ->
    Modules = module_names(Bundles),
    case lists:member(retinue_daemon, Modules) andalso
         lists:member(retinue_protocol, Modules) of
        true -> persistent_term:put(?BUILD_ID_KEY, BuildIdentity);
        false -> ok
    end.

apply_changed(Changed, Unchanged) ->
    case prepare_changed(Changed) of
        {ok, Prepared} -> apply_prepared(Changed, Unchanged, Prepared);
        {error, _Reason} = Error -> Error
    end.

prepare_changed(Changed) ->
    Modules = [{maps:get(module, Bundle), maps:get(filename, Bundle),
                maps:get(binary, Bundle)} || Bundle <- Changed],
    case code:prepare_loading(Modules) of
        {ok, Prepared} -> {ok, Prepared};
        {error, Reasons} -> {error, {prepare_failed, Reasons}}
    end.

apply_prepared(Changed, Unchanged, Prepared) ->
    Modules = module_names(Changed),
    Stateful = stateful_processes(Modules),
    OldVersions = maps:from_list([{Module, module_vsn(Module)} || Module <- Modules]),
    case suspend_all(Stateful, []) of
        {ok, Suspended} ->
            Result = try
                persistent_term:put(?PENDING_KEY,
                                    [{Name, Module, whereis(Name)}
                                     || {Name, Module} <- Stateful]),
                case code:finish_loading(Prepared) of
                    ok ->
                        case change_code_all(Stateful, OldVersions, []) of
                            {ok, Migrated} ->
                                case ensure_supervised_children() of
                                    ok ->
                                        {ok, #{loaded => Modules,
                                               unchanged => module_names(Unchanged),
                                               migrated => Migrated}};
                                    {error, Reason} -> {error, {supervisor_reconcile_failed, Reason}}
                                end;
                            {error, _Reason} = Error -> Error
                        end;
                    {error, Reasons} ->
                        persistent_term:erase(?PENDING_KEY),
                        {error, {finish_loading_failed, Reasons}}
                end
            after
                resume_all(Suspended)
            end,
            case recover_pending() of
                ok -> maybe_restart_runtime_workers(Result);
                {error, RecoveryReason} ->
                    {error, {migration_recovery_failed, Result, RecoveryReason}}
            end;
        {error, _Reason} = Error -> Error
    end.

purge_previous_versions([]) -> ok;
purge_previous_versions([#{module := Module} | Rest]) ->
    case code:soft_purge(Module) of
        true -> purge_previous_versions(Rest);
        false ->
            case recover_busy_old_code(Module) of
                ok ->
                    case code:soft_purge(Module) of
                        true -> purge_previous_versions(Rest);
                        false -> {error, {old_code_busy, Module}}
                    end;
                {error, _Reason} -> {error, {old_code_busy, Module}}
            end
    end.

recover_busy_old_code(retinue_local_api) ->
    restart_supervised_child(retinue_local_api);
recover_busy_old_code(_Module) ->
    {error, unsupported_busy_module}.

maybe_restart_runtime_workers({ok, Result = #{loaded := Loaded}}) ->
    case runtime_restart_required(Loaded) of
        true ->
            case restart_supervised_child(retinue_local_api) of
                ok -> {ok, Result};
                {error, Reason} -> {error, {runtime_restart_failed, retinue_local_api, Reason}}
            end;
        false -> {ok, Result}
    end;
maybe_restart_runtime_workers(Result) -> Result.

runtime_restart_required(Loaded) ->
    lists:any(fun(Module) -> lists:member(Module, Loaded) end,
              [retinue_local_api, retinue_local_protocol]).

restart_supervised_child(Child) ->
    case whereis(retinue_sup) of
        undefined -> ok;
        _Pid -> retinue_sup:restart_child(Child)
    end.

suspend_all([], Suspended) ->
    {ok, lists:reverse(Suspended)};
suspend_all([{Name, Module} = Process | Rest], Suspended) ->
    case safe_sys_call(fun() -> sys:suspend(Name, ?SYS_TIMEOUT_MS) end) of
        ok -> suspend_all(Rest, [Process | Suspended]);
        {error, Reason} ->
            resume_all(Suspended),
            {error, {suspend_failed, Module, Reason}}
    end.

change_code_all([], _OldVersions, Migrated) ->
    {ok, lists:reverse(Migrated)};
change_code_all([{Name, Module} | Rest], OldVersions, Migrated) ->
    OldVsn = maps:get(Module, OldVersions, undefined),
    case safe_sys_call(
           fun() -> sys:change_code(Name, Module, OldVsn, hot_update, ?SYS_TIMEOUT_MS) end) of
        ok ->
            [_Done | Pending] = persistent_term:get(?PENDING_KEY),
            persistent_term:put(?PENDING_KEY, Pending),
            change_code_all(Rest, OldVersions, [Module | Migrated]);
        {error, Reason} -> {error, {code_change_failed, Module, Reason}}
    end.

recover_pending() ->
    recover_pending(persistent_term:get(?PENDING_KEY, [])).

recover_pending([]) ->
    persistent_term:erase(?PENDING_KEY),
    ok;
recover_pending([{Name, Module, OriginalPid} | Rest]) ->
    Current = whereis(Name),
    Result = case Current =/= OriginalPid andalso is_pid(Current) of
        true -> ok;
        false ->
            case whereis(retinue_sup) of
                undefined -> {error, {migration_requires_restart, Module}};
                _ -> restart_supervised_child(Name)
            end
    end,
    case Result of
        ok ->
            persistent_term:put(?PENDING_KEY, Rest),
            recover_pending(Rest);
        {error, _} = Error -> Error
    end.

resume_all(Suspended) ->
    lists:foreach(
      fun({Name, _Module}) ->
          _ = safe_sys_call(fun() -> sys:resume(Name, ?SYS_TIMEOUT_MS) end),
          ok
      end,
      lists:reverse(Suspended)),
    ok.

safe_sys_call(Fun) ->
    try Fun() of
        ok -> ok;
        Other -> {error, Other}
    catch
        Class:Reason -> {error, {Class, Reason}}
    end.

stateful_processes(Modules) ->
    [{Name, Module} || {Name, Module} <- stateful_candidates(),
                       lists:member(Module, Modules),
                       whereis(Name) =/= undefined].

stateful_candidates() ->
    [{Name, Module} || #{id := Name, type := worker, modules := Modules}
                          <- retinue_sup:worker_specs(), Module <- Modules].

bundle_changed(#{module := Module, binary := Binary}) ->
    case {loaded_md5(Module), beam_md5(Binary)} of
        {Md5, {ok, Module, Md5}} when Md5 =/= undefined -> false;
        _ -> true
    end.

loaded_md5(Module) ->
    try Module:module_info(md5)
    catch error:undef -> undefined
    end.

beam_md5(Binary) ->
    case beam_lib:md5(Binary) of
        {ok, {Module, Md5}} -> {ok, Module, Md5};
        {error, Reason} -> {error, Reason}
    end.

beam_module(Binary) ->
    case beam_lib:chunks(Binary, [attributes]) of
        {ok, {Module, _Chunks}} -> {ok, Module};
        {error, _Module, Reason} -> {error, Reason};
        {error, Reason} -> {error, Reason}
    end.

module_vsn(Module) ->
    try proplists:get_value(vsn, Module:module_info(attributes), undefined)
    catch error:undef -> undefined
    end.

module_names(Bundles) ->
    [maps:get(module, Bundle) || Bundle <- Bundles].

ensure_supervised_children() ->
    case whereis(retinue_sup) of
        undefined -> ok;
        _Pid -> retinue_sup:ensure_children()
    end.

%%%-------------------------------------------------------------------
%% Durable build groups and independently replaceable source caches.
%%%-------------------------------------------------------------------
-module(wf_build_store).

-export([load/0, save/1, save_cache/1, path/0, cache_path/0, empty/0]).

-define(SCHEMA, 3).
-define(CACHE_KEYS, [catalogs, revisions, latest, results]).

-doc "Load the build repository; a missing file is an empty repository.".
-spec load() -> {ok, map()} | {error, term()}.
load() ->
    case read(path()) of
        {ok, Store = #{schema := Schema, goals := Goals}}
          when (Schema =:= 1 orelse Schema =:= 2 orelse Schema =:= ?SCHEMA),
               is_map(Goals) ->
            {ok, maps:merge(normalize(Store), load_cache())};
        {ok, _Other} -> {error, unsupported_build_store};
        {error, enoent} -> {ok, maps:merge(empty(), load_cache())};
        {error, Reason} -> {error, Reason}
    end.

-doc "Persist user groups before acknowledging a mutation.".
-spec save(map()) -> ok | {error, term()}.
save(#{goals := Goals}) when is_map(Goals) ->
    write(path(), #{schema => ?SCHEMA, goals => Goals}, [sync]).

-spec save_cache(map()) -> ok | {error, term()}.
save_cache(Store) ->
    write(cache_path(), (maps:with(?CACHE_KEYS, Store))#{schema => 1}, []).

load_cache() ->
    case read(cache_path()) of
        {ok, Cache = #{schema := 1}} ->
            cache_fields(Cache);
        _ -> #{}
    end.

read(Path) ->
    case file:read_file(Path) of
        {ok, Binary} ->
            try binary_to_term(Binary, [safe]) of
                Store -> {ok, Store}
            catch error:Reason -> {error, {invalid_build_store, Reason}}
            end;
        Error -> Error
    end.

write(Path, Store, Modes) ->
    case filelib:ensure_dir(Path) of
        ok ->
            Temp = Path ++ ".tmp",
            Data = term_to_binary(Store, [compressed, deterministic]),
            case file:open(Temp, [write, raw, binary | Modes]) of
                {ok, File} ->
                    Written = case file:change_mode(Temp, 8#600) of
                        ok -> file:write(File, Data);
                        PermissionError -> PermissionError
                    end,
                    Closed = file:close(File),
                    case {Written, Closed} of
                        {ok, ok} -> file:rename(Temp, Path);
                        {{error, _} = WriteError, _} -> WriteError;
                        {_, {error, _} = CloseError} -> CloseError
                    end;
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end.

-doc "Return configured build repository path.".
-spec path() -> file:filename_all().
path() ->
    case application:get_env(retinue_daemon, build_store_file) of
        {ok, Path} -> Path;
        undefined -> wf_paths:data_file("builds.term")
    end.

-spec cache_path() -> file:filename_all().
cache_path() ->
    application:get_env(retinue_daemon, build_cache_file,
                        wf_paths:cache_file("builds.term")).

-doc "Return an empty versioned repository.".
-spec empty() -> map().
empty() ->
    #{schema => ?SCHEMA, catalogs => #{}, revisions => #{}, latest => #{},
      goals => #{}, results => #{}}.

normalize(Store) ->
    maps:merge(empty(), (cache_fields(Store))#{goals => maps:get(goals, Store)}).

cache_fields(Store) ->
    maps:filter(fun(Key, Value) ->
                    lists:member(Key, ?CACHE_KEYS) andalso is_map(Value)
                end, Store).

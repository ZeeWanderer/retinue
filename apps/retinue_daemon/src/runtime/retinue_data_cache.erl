%%%-------------------------------------------------------------------
%% Shared cache/fetch helper for binary-backed data sources.
%%%-------------------------------------------------------------------
-module(retinue_data_cache).

-export([load/1]).

-include_lib("kernel/include/file.hrl").

-define(LOCK_TIMEOUT, 120).
-define(LOCK_RETRIES, 30).
-define(LOCK_SLEEP_MS, 100).

-type opts() :: map().
-type source() :: cached | fetched.

-doc "Load decoded data from cache when fresh; otherwise fetch, decode, and atomically cache it.".
-spec load(opts()) -> {ok, term(), source()} | {error, term()}.
load(Opts) ->
    Cache = maps:get(cache, Opts),
    Ttl = maps:get(ttl, Opts, 60),
    Force = maps:get(refresh, Opts, false),
    FetchFun = maps:get(fetch_fun, Opts),
    DecodeFun = maps:get(decode_fun, Opts),
    NowFun = maps:get(now_fun, Opts, fun now_seconds/0),
    LockTimeout = maps:get(lock_timeout, Opts, ?LOCK_TIMEOUT),
    LockRetries = maps:get(lock_retries, Opts, ?LOCK_RETRIES),
    LockSleep = maps:get(lock_sleep_ms, Opts, ?LOCK_SLEEP_MS),
    _ = ensure_dir(filename:dirname(Cache)),
    with_lock(Cache, NowFun, LockTimeout, LockRetries, LockSleep, fun() ->
        case use_cache(Cache, Ttl, Force, NowFun) of
            true ->
                case read_and_decode(Cache, DecodeFun) of
                    {ok, Data} -> {ok, Data, cached};
                    _ -> fetch_and_cache(Cache, FetchFun, DecodeFun)
                end;
            false ->
                fetch_and_cache(Cache, FetchFun, DecodeFun)
        end
    end).

fetch_and_cache(Cache, FetchFun, DecodeFun) ->
    case FetchFun() of
        {ok, Bin} ->
            case DecodeFun(Bin) of
                {ok, Data} ->
                    _ = write_atomic(Cache, Bin),
                    {ok, Data, fetched};
                Error -> Error
            end;
        Other -> Other
    end.

read_and_decode(Cache, DecodeFun) ->
    case file:read_file(Cache) of
        {ok, Bin} -> DecodeFun(Bin);
        Error -> Error
    end.

use_cache(Cache, Ttl, Force, NowFun) ->
    case Force orelse not filelib:is_file(Cache) of
        true -> false;
        false ->
            case file:read_file_info(Cache, [{time, universal}]) of
                {ok, Info} ->
                    Now = NowFun(),
                    Mtime = calendar:datetime_to_gregorian_seconds(Info#file_info.mtime),
                    Age = abs(Now - Mtime),
                    Age =< Ttl;
                _ -> false
            end
    end.

now_seconds() ->
    calendar:datetime_to_gregorian_seconds(calendar:universal_time()).

ensure_dir(undefined) -> ok;
ensure_dir(Dir) ->
    case filelib:is_dir(Dir) of
        true -> ok;
        false -> filelib:ensure_dir(filename:join(Dir, "wfcli"))
    end.

with_lock(Cache, NowFun, LockTimeout, LockRetries, LockSleep, Fun) ->
    Lock = Cache ++ ".lock",
    case acquire_lock(Lock, NowFun, LockTimeout, LockRetries, LockSleep) of
        ok ->
            try Fun()
            after
                _ = file:delete(Lock)
            end;
        {error, _} = Error ->
            Error
    end.

acquire_lock(_Lock, _NowFun, _LockTimeout, 0, _LockSleep) ->
    {error, lock_timeout};
acquire_lock(Lock, NowFun, LockTimeout, Attempts, LockSleep) ->
    case file:open(Lock, [write, exclusive]) of
        {ok, Io} ->
            _ = file:close(Io),
            ok;
        {error, eexist} ->
            case lock_is_stale(Lock, NowFun, LockTimeout) of
                true ->
                    _ = file:delete(Lock),
                    acquire_lock(Lock, NowFun, LockTimeout, Attempts - 1, LockSleep);
                false ->
                    timer:sleep(LockSleep),
                    acquire_lock(Lock, NowFun, LockTimeout, Attempts - 1, LockSleep)
            end;
        {error, _} ->
            timer:sleep(LockSleep),
            acquire_lock(Lock, NowFun, LockTimeout, Attempts - 1, LockSleep)
    end.

lock_is_stale(Lock, NowFun, LockTimeout) ->
    case file:read_file_info(Lock, [{time, universal}]) of
        {ok, Info} ->
            Now = NowFun(),
            Mtime = calendar:datetime_to_gregorian_seconds(Info#file_info.mtime),
            Age = abs(Now - Mtime),
            Age > LockTimeout;
        _ -> true
    end.

write_atomic(Path, Bin) ->
    Tmp = Path ++ ".tmp",
    case file:write_file(Tmp, Bin) of
        ok ->
            case file:rename(Tmp, Path) of
                ok -> ok;
                {error, Reason} ->
                    _ = file:delete(Tmp),
                    {error, Reason}
            end;
        Error -> Error
    end.

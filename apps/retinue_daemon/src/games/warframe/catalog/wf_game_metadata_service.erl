%%%-------------------------------------------------------------------
%% Persisted game-build metadata observed by wfcompanion.
%%%-------------------------------------------------------------------
-module(wf_game_metadata_service).

-behaviour(gen_server).

-export([start_link/0, snapshot/0, publish/1, clear/0, status/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2,
         terminate/2, code_change/3]).

-define(SERVER, ?MODULE).
-define(CACHE_VERSION, 2).

-type snapshot() :: #{revision := non_neg_integer(),
                      updated_at := integer() | undefined,
                      data := map()}.
-type state() :: #{cache_path := file:filename_all(), snapshot := snapshot()}.

-doc "Start daemon-owned game metadata store.".
-spec start_link() -> {ok, pid()} | ignore | {error, term()}.
start_link() ->
    gen_server:start_link({local, ?SERVER}, ?MODULE, [], []).

-doc "Return current persisted game metadata.".
-spec snapshot() -> snapshot().
snapshot() ->
    gen_server:call(?SERVER, snapshot).

-doc "Store a capture, retaining the last successful catalog when capture fails.".
-spec publish(map()) -> {ok, snapshot()} | {error, term()}.
publish(Data) ->
    gen_server:call(?SERVER, {publish, Data}).

-doc "Clear captured game metadata.".
-spec clear() -> ok | {error, term()}.
clear() ->
    gen_server:call(?SERVER, clear).

-doc "Return metadata revision, executable identity, and cache path.".
-spec status() -> map().
status() ->
    gen_server:call(?SERVER, status).

-spec init([]) -> {ok, state()}.
init([]) ->
    Path = cache_path(),
    {ok, #{cache_path => Path, snapshot => load_snapshot(Path)}}.

-spec handle_call(term(), gen_server:from(), state()) -> {reply, term(), state()}.
handle_call(snapshot, _From, State) ->
    {reply, maps:get(snapshot, State), State};
handle_call({publish, Data}, _From, State) when is_map(Data) ->
    case validate(Data) of
        ok -> publish_data(Data, State);
        {error, _Reason} = Error -> {reply, Error, State}
    end;
handle_call({publish, _Data}, _From, State) ->
    {reply, {error, invalid_game_metadata}, State};
handle_call(clear, _From, State) ->
    replace(#{}, State, ok);
handle_call(status, _From, State) ->
    Snapshot = maps:get(snapshot, State),
    Data = maps:get(data, Snapshot),
    Executable = maps:get(<<"executable">>, Data, #{}),
    Pools = case Data of
        #{<<"archimedea">> := #{<<"catalog">> := Catalog}} when is_map(Catalog) ->
            maps:from_list([{Kind, length(Items)} || {Kind, Items} <- maps:to_list(Catalog), is_list(Items)]);
        _ -> #{}
    end,
    {reply, #{revision => maps:get(revision, Snapshot),
              updated_at => maps:get(updated_at, Snapshot),
              executable_sha256 => maps:get(<<"sha256">>, Executable, undefined),
              available => map_size(Pools) > 0,
              pools => Pools,
              capture_error => maps:get(<<"capture_error">>, Data,
                                        maps:get(<<"unavailable">>, Data, undefined)),
              cache_path => maps:get(cache_path, State)}, State};
handle_call(Request, _From, State) ->
    {reply, {error, {unknown_request, Request}}, State}.

-spec handle_cast(term(), state()) -> {noreply, state()}.
handle_cast(_Message, State) ->
    {noreply, State}.

-spec handle_info(term(), state()) -> {noreply, state()}.
handle_info(_Message, State) ->
    {noreply, State}.

-spec terminate(term(), state()) -> ok.
terminate(_Reason, _State) -> ok.

-spec code_change(term(), state(), term()) -> {ok, state()}.
code_change(_OldVsn, State, _Extra) -> {ok, State}.

publish_data(Capture, State) ->
    Snapshot = maps:get(snapshot, State),
    Previous = maps:get(data, Snapshot),
    Data = retain_catalog(Capture, Previous),
    case Previous =:= Data of
        true -> {reply, {ok, Snapshot}, State};
        false -> replace(Data, State, snapshot)
    end.

retain_catalog(#{<<"unavailable">> := #{<<"reason">> := Reason},
                 <<"executable">> := Executable} = Capture,
               #{<<"archimedea">> := Archimedea} = Previous)
  when is_map(Archimedea), not is_map_key(<<"archimedea">>, Capture) ->
    Previous#{<<"capture_error">> => #{<<"reason">> => Reason,
                                      <<"executable">> => Executable}};
retain_catalog(Capture, _Previous) -> Capture.

replace(Data, State, Reply) ->
    Old = maps:get(snapshot, State),
    Snapshot = #{revision => maps:get(revision, Old) + 1,
                 updated_at => erlang:system_time(millisecond),
                 data => Data},
    case persist_snapshot(maps:get(cache_path, State), Snapshot) of
        ok ->
            Result = case Reply of snapshot -> {ok, Snapshot}; _ -> Reply end,
            {reply, Result, State#{snapshot => Snapshot}};
        {error, Reason} ->
            {reply, {error, {game_metadata_cache_write_failed, Reason}}, State}
    end.

validate(#{<<"schema">> := 2, <<"executable">> := Executable} = Data)
  when is_map(Executable) ->
    case maps:get(<<"sha256">>, Executable, undefined) of
        Hash when is_binary(Hash), byte_size(Hash) =:= 64 ->
            case validate_hash(Hash) of
                ok -> validate_payload(Data);
                {error, _Reason} = Error -> Error
            end;
        _ -> {error, invalid_game_metadata_executable}
    end;
validate(_Data) ->
    {error, invalid_game_metadata}.

validate_payload(#{<<"archimedea">> := Archimedea}) when is_map(Archimedea) -> ok;
validate_payload(#{<<"unavailable">> := #{<<"reason">> := Reason}}) when is_binary(Reason) -> ok;
validate_payload(_Data) -> {error, invalid_game_metadata}.

validate_hash(Hash) ->
    case lists:all(fun is_hex/1, binary_to_list(Hash)) of
        true -> ok;
        false -> {error, invalid_game_metadata_executable}
    end.

is_hex(Char) ->
    (Char >= $0 andalso Char =< $9) orelse (Char >= $a andalso Char =< $f).

cache_path() ->
    case application:get_env(retinue_daemon, game_metadata_cache) of
        {ok, Path} -> Path;
        undefined -> wf_paths:cache_file("game-metadata.term")
    end.

empty_snapshot() ->
    #{revision => 0, updated_at => undefined, data => #{}}.

load_snapshot(Path) ->
    case file:read_file(Path) of
        {ok, Binary} ->
            try binary_to_term(Binary, [safe]) of
                #{version := ?CACHE_VERSION, snapshot := Snapshot}
                  when is_map(Snapshot) -> normalize_snapshot(Snapshot);
                _ -> empty_snapshot()
            catch _:_ -> empty_snapshot()
            end;
        {error, _Reason} -> empty_snapshot()
    end.

normalize_snapshot(Snapshot) ->
    #{revision => maps:get(revision, Snapshot, 0),
      updated_at => maps:get(updated_at, Snapshot, undefined),
      data => maps:get(data, Snapshot, #{})}.

persist_snapshot(Path, Snapshot) ->
    case filelib:ensure_dir(Path) of
        ok ->
            Temp = Path ++ ".tmp",
            Binary = term_to_binary(#{version => ?CACHE_VERSION, snapshot => Snapshot},
                                    [compressed]),
            case file:write_file(Temp, Binary) of
                ok ->
                    _ = file:change_mode(Temp, 8#600),
                    file:rename(Temp, Path);
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end.

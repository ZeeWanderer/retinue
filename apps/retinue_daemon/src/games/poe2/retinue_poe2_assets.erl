-module(retinue_poe2_assets).
-behaviour(gen_server).
-include_lib("kernel/include/file.hrl").
-export([start_link/0, start_link/1, resolve/1, resolve/2, load/2, load/3, index/1, image_path/1, png/1]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

start_link() -> start_link(#{}).
start_link(Options) ->
    case maps:get(name, Options, ?MODULE) of
        undefined -> gen_server:start_link(?MODULE, Options, []);
        Name -> gen_server:start_link({local, Name}, ?MODULE, Options, [])
    end.
resolve(Id) -> resolve(?MODULE, Id).
resolve(Server, Id) -> gen_server:call(Server, {resolve, Id}, 35000).

init(Options) ->
    process_flag(trap_exit, true),
    {ok, #{loader => maps:get(loader, Options, fun ?MODULE:load/2),
           entries => #{}, workers => #{}, pending => #{}, queue => queue:new()}}.

handle_call({resolve, Id}, From, State) when is_binary(Id), byte_size(Id) > 0, byte_size(Id) =< 128 ->
    Entries = maps:get(entries, State),
    Now = erlang:monotonic_time(second),
    case maps:get(Id, Entries, undefined) of
        #{reply := Reply, expires := Expires} when Expires > Now -> {reply, Reply, State};
        #{reply := {ok, _} = Reply} -> {reply, Reply, enqueue(Id, none, State)};
        _ ->
            Pending = maps:get(pending, State),
            Waiters = maps:get(Id, Pending, []),
            case map_size(Pending) < 64 andalso length(Waiters) < 16 of
                true -> {noreply, enqueue(Id, From, State)};
                false -> {reply, {error, asset_cache_busy}, State}
            end
    end;
handle_call(_, _, State) -> {reply, {error, invalid_currency}, State}.
handle_cast(_, State) -> {noreply, State}.

enqueue(Id, From, State = #{pending := Pending, queue := Queue}) ->
    Waiters = case From of none -> []; _ -> [From] end,
    case maps:find(Id, Pending) of
        {ok, Old} -> State#{pending => Pending#{Id => Waiters ++ Old}};
        error when map_size(Pending) < 64 -> dispatch(State#{pending => Pending#{Id => Waiters}, queue => queue:in(Id, Queue)});
        error -> State
    end.

dispatch(State = #{workers := Workers, queue := Queue, loader := Loader}) when map_size(Workers) < 2 ->
    case queue:out(Queue) of
        {empty, _} -> State;
        {{value, Id}, Rest} ->
            Parent = self(),
            {Pid, Ref} = spawn_opt(fun() -> Parent ! {loaded, self(), Loader(Id, Parent)} end, [link, monitor]),
            Timer = erlang:send_after(30000, self(), {timeout, Pid}),
            dispatch(State#{queue => Rest, workers => Workers#{Pid => {Ref, Timer, Id}}})
    end;
dispatch(State) -> State.

handle_info({cached, Pid, Data}, State = #{workers := Workers}) ->
    case maps:find(Pid, Workers) of
        {ok, {_, _, Id}} ->
            Entries = maps:get(entries, State),
            Entry = maps:get(Id, Entries, #{}),
            Next = State#{entries => Entries#{Id => Entry#{reply => {ok, Data}, expires => erlang:monotonic_time(second)}}},
            {noreply, complete_waiters(Id, {ok, Data}, Next)};
        error -> {noreply, State}
    end;
handle_info({loaded, Pid, Reply}, State) -> {noreply, finish(Pid, Reply, State)};
handle_info({'DOWN', Ref, process, Pid, Reason}, State = #{workers := Workers}) ->
    case maps:find(Pid, Workers) of
        {ok, {Ref, _, _}} -> {noreply, finish(Pid, {error, {asset_worker_failed, Reason}}, State)};
        _ -> {noreply, State}
    end;
handle_info({timeout, Pid}, State = #{workers := Workers}) ->
    case maps:is_key(Pid, Workers) of true -> exit(Pid, kill); false -> ok end,
    {noreply, finish(Pid, {error, asset_timeout}, State)};
handle_info(_, State) -> {noreply, State}.

complete_waiters(Id, Reply, State = #{pending := Pending}) ->
    lists:foreach(fun(From) -> gen_server:reply(From, Reply) end, maps:get(Id, Pending, [])),
    State#{pending => Pending#{Id => []}}.

finish(Pid, Reply0, State = #{workers := Workers, entries := Entries}) ->
    case maps:take(Pid, Workers) of
        error -> State;
        {{Ref, Timer, Id}, Rest} ->
            erlang:demonitor(Ref, [flush]),
            erlang:cancel_timer(Timer),
            TTL = case Reply0 of {ok, _} -> 3600; _ -> 300 end,
            Reply = case {Reply0, maps:get(Id, Entries, undefined)} of
                {{error, Reason}, #{reply := {ok, _} = Cached}} when Reason =/= unknown_currency -> Cached;
                _ -> Reply0
            end,
            Bounded = case map_size(Entries) >= 256 andalso not maps:is_key(Id, Entries) of
                true -> [{_, Oldest} | _] = lists:sort([{maps:get(expires, V), K} || {K, V} <- maps:to_list(Entries)]),
                        maps:remove(Oldest, Entries);
                false -> Entries
            end,
            Error = case Reply0 of {error, Why} -> Why; _ -> undefined end,
            PreviousError = maps:get(last_error, maps:get(Id, Entries, #{}), undefined),
            case Error =/= undefined andalso Error =/= PreviousError of
                true -> logger:warning(#{event => poe2_currency_asset_failed, currency => Id, reason => Error});
                false -> ok
            end,
            Next = complete_waiters(Id, Reply, State),
            dispatch(Next#{workers => Rest, pending => maps:remove(Id, maps:get(pending, Next)),
                entries => Bounded#{Id => #{reply => Reply, last_error => Error,
                                            expires => erlang:monotonic_time(second) + TTL}}})
    end.

terminate(_, #{workers := Workers}) ->
    maps:foreach(fun(Pid, _) -> exit(Pid, shutdown) end, Workers), ok.
code_change(_, State, _) -> {ok, State#{loader => fun ?MODULE:load/2}}.

load(Id, Parent) -> load(Id, Parent, #{}).
load(Id, Parent, Options) ->
    Root = maps:get(root, Options, retinue_paths:game_file(cache, poe2, "assets/currencies")),
    Fetch = maps:get(catalog, Options,
        fun() -> body(retinue_poe2_trade_http:request(get, <<"/api/trade2/data/static">>, [], <<>>)) end),
    ImageHTTP = maps:get(image, Options, fun(Path) -> body(retinue_poe2_trade_http:image(Path)) end),
    Cached = cached_catalog_image(Id, Root),
    case Cached of {ok, Data} -> Parent ! {cached, self(), Data}; _ -> ok end,
    case retinue_data_cache:load(#{cache => filename:join(Root, "static.json"), ttl => 86400,
            fetch_fun => Fetch,
            decode_fun => fun decode/1}) of
        {ok, Catalog, Source} ->
            case Source of fetched -> prune_catalog(Root, Catalog); _ -> ok end,
            case maps:find(Id, Catalog) of
                {ok, Entry} -> load_image(Entry, Root, ImageHTTP);
                error -> {error, unknown_currency}
            end;
        Error -> Error
    end.

load_image(Entry = #{<<"image">> := Image}, Root, ImageHTTP) ->
    Meta = source_file(Root, Image),
    Fetch = fun() ->
        case ImageHTTP(Image) of
            {ok, Bytes} ->
                case png(Bytes) of
                    {ok, Digest} ->
                        File = object(Root, Digest),
                        ok = filelib:ensure_dir(File),
                        Temp = File ++ "." ++ integer_to_list(erlang:unique_integer([positive])) ++ ".tmp",
                        case file:write_file(Temp, Bytes) of
                            ok ->
                                case file:rename(Temp, File) of
                                    ok -> {ok, iolist_to_binary(json:encode(#{<<"image">> => Image, <<"digest">> => Digest,
                                              <<"media_type">> => <<"image/png">>, <<"byte_size">> => byte_size(Bytes)}))};
                                    Error -> _ = file:delete(Temp), Error
                                end;
                            Error -> Error
                        end;
                    Error -> Error
                end;
            Error -> Error
        end
    end,
    case retinue_data_cache:load(#{cache => Meta, ttl => 604800,
            fetch_fun => Fetch, decode_fun => fun(Bytes) -> descriptor(Bytes, Root, Image) end}) of
        {ok, Data, Source} ->
            case Source of fetched -> prune(Root); _ -> ok end,
            {ok, maps:merge(Data, Entry)};
        Error -> Error
    end.

object(Root, Digest) -> filename:absname(filename:join([Root, "objects", binary_to_list(Digest) ++ ".png"])).

source_file(Root, Image) ->
    Key = binary_to_list(binary:encode_hex(crypto:hash(sha256, Image), lowercase)),
    filename:join([Root, "sources", Key ++ ".json"]).

cached_catalog_image(Id, Root) ->
    try
        {ok, CatalogBytes} = read_bounded(filename:join(Root, "static.json"), 8388608),
        {ok, Catalog} = decode(CatalogBytes),
        #{<<"image">> := Image} = Entry = maps:get(Id, Catalog),
        {ok, Bytes} = read_bounded(source_file(Root, Image), 8192),
        {ok, Data} = descriptor(Bytes, Root, Image),
        {ok, maps:merge(Data, Entry)}
    catch _:_ -> {error, no_cached_image} end.

descriptor(Bytes, Root, ImagePath) ->
    try
        true = byte_size(Bytes) =< 8192,
        Data = json:decode(Bytes),
        ImagePath = maps:get(<<"image">>, Data),
        Digest = maps:get(<<"digest">>, Data),
        true = byte_size(Digest) =:= 64 andalso re:run(Digest, <<"^[a-f0-9]{64}$">>) =/= nomatch,
        File = object(Root, Digest),
        {ok, Image} = read_bounded(File, 2097152),
        {ok, Digest} = png(Image),
        {ok, Data#{<<"path">> => list_to_binary(File)}}
    catch _:_ -> {error, no_cached_image} end.

read_bounded(Path, Limit) ->
    case file:open(Path, [read, raw, binary]) of
        {ok, File} ->
            try
                case file:read(File, Limit + 1) of
                    {ok, Bytes} when byte_size(Bytes) =< Limit -> {ok, Bytes};
                    _ -> {error, invalid_cache_file}
                end
            after file:close(File) end;
        Error -> Error
    end.

prune(Root) ->
    References = lists:filtermap(fun(Path) ->
        try
            {ok, Bytes} = read_bounded(Path, 8192),
            Digest = maps:get(<<"digest">>, json:decode(Bytes)),
            {true, object(Root, Digest)}
        catch _:_ -> false end
    end, filelib:wildcard(filename:join([Root, "sources", "*.json"]))),
    Now = erlang:system_time(second),
    %% Keep superseded pixels for in-flight readers holding their content digest.
    lists:foreach(fun(Path) ->
        case file:read_file_info(Path, [{time, posix}]) of
            {ok, #file_info{mtime = Time}} when Now - Time > 86400 ->
                case lists:member(filename:absname(Path), References) of
                    true -> ok;
                    false -> _ = file:delete(Path)
                end;
            _ -> ok
        end
    end, filelib:wildcard(filename:join([Root, "objects", "*.png"]))).

prune_catalog(Root, Catalog) ->
    Sources = maps:from_list([{source_file(Root, maps:get(<<"image">>, E)), true} || E <- maps:values(Catalog)]),
    lists:foreach(fun(Path) ->
        case maps:is_key(Path, Sources) of true -> ok; false -> _ = file:delete(Path) end
    end, filelib:wildcard(filename:join([Root, "sources", "*.json"]))),
    prune(Root).

body({ok, 200, _, Bytes}) -> {ok, Bytes};
body({ok, Code, _, _}) -> {error, {http_status, Code}};
body(Error) -> Error.

decode(Bytes) ->
    try index(json:decode(Bytes)) catch _:_ -> {error, invalid_currency_catalog} end.

index(#{<<"result">> := Groups}) when is_list(Groups), length(Groups) =< 128 ->
    Entries = [Entry#{<<"image">> => Path, <<"tier">> => tier(Id)} ||
        #{<<"entries">> := Rows} <- Groups, is_list(Rows), length(Rows) =< 10000,
        #{<<"id">> := Id, <<"text">> := Name, <<"image">> := Image} = Entry <- Rows,
        is_binary(Id), byte_size(Id) > 0, byte_size(Id) =< 128,
        is_binary(Name), byte_size(Name) =< 256,
        {ok, Path} <- [image_path(Image)]],
    case Entries of
        [] -> {error, invalid_currency_catalog};
        _ -> {ok, maps:from_list([{maps:get(<<"id">>, E), maps:with([<<"id">>, <<"text">>, <<"image">>, <<"tier">>], E)} || E <- Entries])}
    end;
index(_) -> {error, invalid_currency_catalog}.

image_path(<<"https://web.poecdn.com", Path/binary>>) -> image_path(Path);
image_path(<<"/gen/image/", _/binary>> = Path) when byte_size(Path) < 2048 -> safe_path(Path);
image_path(<<"/image/", _/binary>> = Path) when byte_size(Path) < 2048 -> safe_path(Path);
image_path(_) -> error.
safe_path(Path) ->
    case binary:match(Path, [<<"..">>, <<"\\">>, <<"\r">>, <<"\n">>, <<"#">>]) of
        nomatch -> {ok, Path};
        _ -> error
    end.

tier(<<"greater-", Rest/binary>>) -> tier(Rest, <<"II">>);
tier(<<"perfect-", Rest/binary>>) -> tier(Rest, <<"III">>);
tier(_) -> null.
tier(Rest, Tier) ->
    case lists:member(Rest, [<<"orb-of-transmutation">>, <<"orb-of-augmentation">>, <<"regal-orb">>,
                              <<"exalted-orb">>, <<"chaos-orb">>]) of true -> Tier; false -> null end.

png(<<137,80,78,71,13,10,26,10, 13:32, "IHDR", W:32, H:32, _/binary>> = Bytes)
  when W > 0, H > 0, W =< 512, H =< 512, byte_size(Bytes) =< 2097152 ->
    {ok, binary:encode_hex(crypto:hash(sha256, Bytes), lowercase)};
png(_) -> {error, invalid_currency_image}.

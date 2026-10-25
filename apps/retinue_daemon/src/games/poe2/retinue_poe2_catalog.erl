-module(retinue_poe2_catalog).
-behaviour(gen_server).
-export([start_link/0, start_link/1, enrich/1, enrich/2, enrich_cached/1, index/1, annotate/2]).
-export([load/1, clock/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

start_link() -> start_link(#{}).
start_link(Options) ->
    case maps:get(name, Options, ?MODULE) of
        undefined -> gen_server:start_link(?MODULE, Options, []);
        Name -> gen_server:start_link({local, Name}, ?MODULE, Options, [])
    end.
enrich(Item) -> enrich(?MODULE, Item).
enrich(Server, Item) ->
    enrich(Server, Item, enrich).
enrich_cached(Item) -> enrich(?MODULE, Item, enrich_cached).
enrich(Server, Item, Mode) ->
    try gen_server:call(Server, {Mode, Item}, 5000)
    catch exit:_ -> retinue_poe2_trade_profiles:prepare(Item#{<<"ranking_status">> => <<"unavailable">>}) end.

init(Options) ->
    process_flag(trap_exit, true),
    {ok, #{loader => maps:get(loader, Options, fun ?MODULE:load/1),
           clock => maps:get(clock, Options, fun ?MODULE:clock/0),
           data => undefined, worker => undefined, waiters => [], refresh => undefined,
           timeout => maps:get(timeout, Options, 30000)}}.

handle_call({enrich_cached, Item}, _, State = #{data := Data}) ->
    Reply = case Data of
        undefined -> unavailable(Item);
        _ -> annotate(Item, Data)
    end,
    {reply, Reply, State};
handle_call({enrich, Item}, From, State) ->
    Next = refresh(State),
    case maps:get(data, Next) of
        Data when is_map(Data) -> {reply, annotate(Item, Data), Next};
        undefined ->
            Waiters = maps:get(waiters, Next),
            case maps:get(worker, Next) =/= undefined andalso length(Waiters) < 16 of
                true -> {noreply, Next#{waiters => [{From, Item} | Waiters]}};
                false -> {reply, unavailable(Item), Next}
            end
    end;
handle_call(_, _, State) -> {reply, {error, unsupported_request}, State}.
handle_cast(_, State) -> {noreply, State}.

refresh(State = #{worker := undefined, refresh := Refresh, loader := Loader, clock := Clock}) ->
    case Refresh =:= undefined orelse Clock() >= Refresh of
        true ->
            Parent = self(),
            {Pid, Ref} = spawn_opt(fun() -> Parent ! {catalog_loaded, self(), Loader(Parent)} end, [link, monitor]),
            Timer = erlang:send_after(maps:get(timeout, State), self(), {load_timeout, Pid}),
            State#{worker => {Pid, Ref}, timer => Timer};
        false -> State
    end;
refresh(State) -> State.

handle_info({catalog_cached, Pid, Data}, State = #{worker := {Pid, _}}) ->
    reply_waiters(State, fun(Item) -> annotate(Item, Data) end),
    {noreply, State#{data => Data, waiters => []}};
handle_info({catalog_loaded, Pid, {ok, Data, Source}}, State = #{worker := {Pid, _}, clock := Clock}) ->
    finish_worker(State),
    reply_waiters(State, fun(Item) -> annotate(Item, Data) end),
    Delay = case Source of stale -> 300; _ -> 3600 end,
    {noreply, State#{data => Data, worker => undefined, waiters => [], refresh => Clock() + Delay}};
handle_info({catalog_loaded, Pid, Error}, State = #{worker := {Pid, _}}) -> failed(Error, State);
handle_info({'DOWN', Ref, process, Pid, Reason}, State = #{worker := {Pid, Ref}}) -> failed(Reason, State);
handle_info({load_timeout, Pid}, State = #{worker := {Pid, _}}) ->
    exit(Pid, kill),
    {noreply, State};
handle_info(_, State) -> {noreply, State}.

finish_worker(#{worker := {_, Ref}, timer := Timer}) ->
    erlang:demonitor(Ref, [flush]),
    erlang:cancel_timer(Timer).

failed(Reason, State = #{clock := Clock}) ->
    finish_worker(State),
    logger:warning("poe2 ranking catalog unavailable: ~p", [Reason]),
    reply_waiters(State, fun unavailable/1),
    {noreply, State#{worker => undefined, waiters => [], refresh => Clock() + 300}}.

reply_waiters(#{waiters := Waiters}, Reply) ->
    lists:foreach(fun({From, Item}) -> gen_server:reply(From, Reply(Item)) end, Waiters).
unavailable(Item) -> retinue_poe2_trade_profiles:prepare(Item#{<<"ranking_status">> => <<"unavailable">>}).
terminate(_, #{worker := {Pid, _}}) -> exit(Pid, shutdown), ok;
terminate(_, _) -> ok.
code_change(_, State, _) ->
    Defaults = #{loader => fun ?MODULE:load/1, clock => fun ?MODULE:clock/0},
    {ok, maps:fold(fun(Key, Default, Acc) ->
        case erlang:fun_info(maps:get(Key, Acc), module) of
            {module, ?MODULE} -> Acc#{Key => Default};
            _ -> Acc
        end
    end, State, Defaults)}.

clock() -> erlang:monotonic_time(second).

load(Parent) ->
    Cache = retinue_paths:game_file(cache, poe2, "trade/enrichment.json"),
    case file:read_file(Cache) of
        {ok, Cached} ->
            case decode(Cached) of
                {ok, Data0} -> Parent ! {catalog_cached, self(), Data0};
                _ -> ok
            end;
        _ -> ok
    end,
    case retinue_data_cache:load(#{cache => Cache, ttl => 86400,
                                  fetch_fun => fun fetch/0, decode_fun => fun decode/1}) of
        {ok, _, _} = Result -> Result;
        Error ->
            case file:read_file(Cache) of
                {ok, Bytes} ->
                    case decode(Bytes) of
                        {ok, Data} ->
                            logger:warning("poe2 ranking catalog refresh failed; using cached data: ~p", [Error]),
                            {ok, Data, stale};
                        _ -> Error
                    end;
                _ -> Error
            end
    end.

fetch() ->
    try
        Files = [{<<"bases">>, <<"base_items.min.json">>}, {<<"mods">>, <<"mods.min.json">>},
                 {<<"stats">>, <<"stat_translations/stat_descriptions.min.json">>}],
        Data = maps:from_list([{Key, fetch_file(File)} || {Key, File} <- Files]),
        {ok, iolist_to_binary(json:encode(Data#{<<"schema">> => 1}))}
    catch throw:Reason -> {error, Reason} end.

fetch_file(File) ->
    case retinue_poe2_trade_http:data(<<"/poe2/", File/binary>>) of
        {ok, 200, _, Bytes} ->
            try json:decode(Bytes)
            catch _:_ -> throw({invalid_catalog_json, File}) end;
        {ok, Status, _, _} -> throw({catalog_http_status, File, Status});
        {error, Reason} -> throw({catalog_fetch_failed, File, Reason})
    end.

decode(Bytes) when byte_size(Bytes) =< 64 * 1024 * 1024 ->
    try
        #{<<"schema">> := 1} = Data = json:decode(Bytes),
        index(Data)
    catch _:_ -> {error, invalid_ranking_catalog} end;
decode(_) -> {error, catalog_too_large}.

index(#{<<"bases">> := Bases, <<"mods">> := Mods, <<"stats">> := Stats}) ->
    try
        true = is_map(Bases) andalso map_size(Bases) > 0 andalso map_size(Bases) =< 100000,
        true = is_map(Mods) andalso map_size(Mods) > 0 andalso map_size(Mods) =< 100000,
        true = is_list(Stats) andalso length(Stats) > 0 andalso length(Stats) =< 100000,
        BaseIndex = lists:foldl(fun(Base, Acc) ->
            Name = maps:get(<<"name">>, Base),
            add(Name, maps:with([<<"item_class">>, <<"tags">>], Base), Acc)
        end, #{}, maps:values(Bases)),
        StatIndex = lists:foldl(fun(#{<<"ids">> := Ids} = Stat, Acc) ->
            lists:foldl(fun(#{<<"id">> := Id}, A) -> add(Id, Ids, A) end, Acc,
                        maps:get(<<"trade_stats">>, Stat, []))
        end, #{}, Stats),
        ModIndex = maps:fold(fun(_, Mod, Acc) ->
            case Mod of
                #{<<"generation_type">> := Type, <<"name">> := Name,
                  <<"stats">> := [_ | _], <<"spawn_weights">> := [_ | _],
                  <<"type">> := Family, <<"required_level">> := _}
                  when Type =:= <<"prefix">>; Type =:= <<"suffix">> ->
                    case maps:get(<<"is_essence_only">>, Mod, false) of
                        true -> Acc;
                        false ->
                            Compact = maps:with([<<"type">>, <<"name">>, <<"stats">>,
                                <<"required_level">>, <<"spawn_weights">>, <<"generation_type">>], Mod),
                            add({name, Name}, Compact, add({family, Family}, Compact, Acc))
                    end;
                _ -> Acc
            end
        end, #{}, Mods),
        {ok, #{bases => BaseIndex, stats => StatIndex, mods => ModIndex}}
    catch _:_ -> {error, invalid_ranking_catalog} end;
index(_) -> {error, invalid_ranking_catalog}.

add(Key, Value, Map) -> Map#{Key => lists:usort([Value | maps:get(Key, Map, [])])}.

annotate(Item, #{bases := Bases, stats := Stats, mods := Mods}) ->
    Base = case maps:get(maps:get(<<"base">>, Item), Bases, []) of [B] -> B; _ -> #{} end,
    Rows = [annotate_row(Row, Item, Base, Stats, Mods) || Row <- maps:get(<<"modifiers">>, Item)],
    Category = case retinue_poe2_trade_profiles:category(maps:get(<<"item_class">>, Item, null)) of
        null -> retinue_poe2_trade_profiles:category(maps:get(<<"item_class">>, Base, null));
        Known -> Known
    end,
    retinue_poe2_trade_profiles:prepare(Item#{<<"modifiers">> => Rows,
        <<"category">> => Category, <<"ranking_status">> => <<"ready">>}).

annotate_row(#{<<"supported">> := true, <<"id">> := Id} = Row, Item, Base, Stats, Mods) ->
    Matches = lists:usort(lists:append([maps:get(Key, Stats, [])
        || Key <- binary:split(Id, <<",">>, [global])])),
    Ids = case Matches of [Known = [_ | _]] -> Known; _ -> [] end,
    Enriched = case Ids of [] -> Row; _ -> Row#{<<"internal_ids">> => Ids} end,
    case tier(Row, Item, Base, Ids, Mods) of
        undefined -> Enriched;
        Tier -> Enriched#{<<"tier_info">> => Tier}
    end;
annotate_row(Row, _, _, _, _) -> Row.

tier(#{<<"decorator">> := #{<<"name">> := Name, <<"tier">> := Current} = Decorator},
     Item, Base, Ids, Mods) ->
    Tags = maps:get(<<"tags">>, Base, []),
    Affix = string:lowercase(maps:get(<<"affix">>, Decorator, <<>>)),
    Matches = [M || M <- maps:get({name, Name}, Mods, []),
        maps:get(<<"generation_type">>, M) =:= Affix, available(M, Tags),
        (Ids =:= [] orelse maps:get(<<"id">>, hd(maps:get(<<"stats">>, M))) =:= hd(Ids))],
    case lists:usort([maps:get(<<"type">>, M) || M <- Matches]) of
        [Family] ->
            Levels = lists:usort([maps:get(<<"required_level">>, M)
                || M <- maps:get({family, Family}, Mods), available(M, Tags),
                   maps:get(<<"generation_type">>, M) =:= Affix]),
            Count = max(length(Levels), Current),
            Available = length([L || L <- Levels, L =< maps:get(<<"item_level">>, Item, 0)]),
            #{<<"current">> => Current, <<"count">> => Count,
              <<"max">> => min(1 + max(Count - Available, 0), Current)};
        _ when Current =:= 1 -> #{<<"current">> => 1, <<"count">> => 1, <<"max">> => 1};
        _ -> #{<<"current">> => Current}
    end;
tier(_, _, _, _, _) -> undefined.

available(Mod, Tags) ->
    Weights = [W || #{<<"tag">> := Tag, <<"weight">> := W} <- maps:get(<<"spawn_weights">>, Mod),
                    lists:member(Tag, Tags)],
    case Weights of [W | _] -> W > 0; [] -> false end.

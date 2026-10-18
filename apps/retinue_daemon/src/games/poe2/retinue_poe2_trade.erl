-module(retinue_poe2_trade).
-behaviour(gen_server).
-export([start_link/0, start_link/1, request/1, request/2, submit/3, clock/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

start_link() -> start_link(#{}).
start_link(Options) ->
    case maps:get(name, Options, ?MODULE) of
        undefined -> gen_server:start_link(?MODULE, Options, []);
        Name -> gen_server:start_link({local, Name}, ?MODULE, Options, [])
    end.
request(Request) -> request(?MODULE, Request).
request(Server, Request) -> gen_server:call(Server, {request, Request}, 45000).
submit(Recipient, Id, Request) -> gen_server:cast(?MODULE, {request, Recipient, Id, Request}).

init(Options) ->
    process_flag(trap_exit, true),
    {ok, #{http => maps:get(http, Options, fun retinue_poe2_trade_http:request/4),
           enrich => maps:get(enrich, Options, fun retinue_poe2_catalog:enrich/1),
           clock => maps:get(clock, Options, fun ?MODULE:clock/0),
           cache => #{}, limits => #{}, results => #{}, active => undefined}}.

handle_call({request, Request0}, From, State) ->
    case validate(Request0) of
        {error, _} = Error -> {reply, Error, State};
        {ok, Request} -> start_request(Request, From, State)
    end;
handle_call(_, _, State) -> {reply, retinue_poe2_trade_query:error(unsupported_request), State}.

start_request(Request, From, State = #{active := undefined}) ->
    Parent = self(),
    {Pid, Ref} = spawn_opt(fun() ->
        {Reply, Next} = execute(Request, State),
        Parent ! {trade_done, self(), Reply, Next}
    end, [link, monitor]),
    Timer = erlang:send_after(42000, self(), {trade_timeout, Pid}),
    {noreply, State#{active => #{pid => Pid, ref => Ref, timer => Timer,
                                request => Request, waiters => [From]}}};
start_request(Request, From, State = #{active := #{request := Request, waiters := Waiters} = Active})
  when length(Waiters) < 16 ->
    {noreply, State#{active => Active#{waiters => [From | Waiters]}}};
start_request(_, _, State) ->
    {reply, retinue_poe2_trade_query:error(trade_busy, 1), State}.

handle_cast({request, Recipient, Id, Request0}, State) ->
    From = {local, Recipient, Id},
    case validate(Request0) of
        {error, _} = Error -> reply(From, Error), {noreply, State};
        {ok, Request} ->
            case start_request(Request, From, State) of
                {reply, Reply, Next} -> reply(From, Reply), {noreply, Next};
                Pending -> Pending
            end
    end;
handle_cast(_, State) -> {noreply, State}.
handle_info({trade_done, Pid, Reply, Next}, State = #{active := #{pid := Pid} = Active}) ->
    finish(Reply, Active),
    Cache = case maps:get(discard_stats, Active, false) of
        true -> maps:remove(stats, maps:get(cache, Next));
        false -> maps:get(cache, Next)
    end,
    {noreply, State#{cache => Cache, limits => maps:get(limits, Next),
                    results => maps:get(results, Next), active => undefined}};
handle_info({trade_timeout, Pid}, State = #{active := #{pid := Pid} = Active}) ->
    exit(Pid, kill),
    failed(upstream_timeout, Active, State);
handle_info({'DOWN', Ref, process, _, _}, State = #{active := #{ref := Ref} = Active}) ->
    failed(upstream_unavailable, Active, State);
handle_info(_, State) -> {noreply, State}.

failed(Reason, Active, State) ->
    finish(retinue_poe2_trade_query:error(Reason, 60), Active),
    Limits = maps:get(limits, State),
    {noreply, State#{active => undefined, limits => Limits#{global => now(State) + 60}}}.

finish(Reply, #{ref := Ref, timer := Timer, waiters := Waiters}) ->
    erlang:cancel_timer(Timer),
    erlang:demonitor(Ref, [flush]),
    lists:foreach(fun(From) -> reply(From, Reply) end, Waiters).

reply(From, {predict, League, Item}) -> retinue_poe2_prediction:submit(From, League, Item);
reply({local, Recipient, Id}, Reply) -> Recipient ! {poe2_trade_result, Id, Reply};
reply(From, Reply) -> gen_server:reply(From, Reply).

terminate(_, #{active := #{pid := Pid}}) -> exit(Pid, shutdown), ok;
terminate(_, _) -> ok.
code_change(_, State0, _) ->
    Active = case maps:get(active, State0) of
        undefined -> undefined;
        Worker -> Worker#{discard_stats => true}
    end,
    State = State0#{cache => maps:remove(stats, maps:get(cache, State0)), active => Active},
    case erlang:fun_info(maps:get(clock, State), module) of
        {module, ?MODULE} -> {ok, State#{clock => fun ?MODULE:clock/0}};
        _ -> {ok, State}
    end.

clock() -> erlang:system_time(second).

validate(#{<<"op">> := <<"trade_leagues">>}) -> {ok, #{<<"op">> => <<"trade_leagues">>}};
validate(#{<<"op">> := Op, <<"text">> := Text} = Request)
  when Op =:= <<"trade_item">>; Op =:= <<"trade_search">>; Op =:= <<"trade_predict">> ->
    case retinue_poe2_trade_item:parse(Text, #{}) of
        {ok, _} -> {ok, maps:with([<<"op">>, <<"text">>, <<"league">>, <<"status">>,
                                   <<"rarity">>, <<"currency">>, <<"indexed">>, <<"item_level">>,
                                   <<"sort">>, <<"base">>, <<"filters">>, <<"conditions">>], Request)};
        {error, Reason} -> retinue_poe2_trade_query:error(Reason)
    end;
validate(_) -> retinue_poe2_trade_query:error(invalid_trade_request).

execute(#{<<"op">> := <<"trade_leagues">>}, State) ->
    case metadata(leagues, State) of
        {ok, Leagues, Next} -> {{ok, league_context(Leagues)}, Next};
        {error, Error, Next} -> {{error, Error}, Next}
    end;
execute(#{<<"op">> := <<"trade_predict">>} = Request, State) ->
    case prepare(Request, State) of
        {ok, Item, Leagues, Next} ->
            League = maps:get(<<"league">>, Request, undefined),
            case lists:any(fun(L) -> maps:get(<<"id">>, L) =:= League end, Leagues) of
                true -> {{predict, League, maps:get(prediction_item, Item)}, Next};
                false -> {retinue_poe2_trade_query:error(invalid_league), Next}
            end;
        {error, Error, Next} -> {{error, Error}, Next}
    end;
execute(Request, State) ->
    case prepare(Request, State) of
        {ok, Item, Leagues, Next} ->
            case metadata(filters, Next) of
                {ok, Options, Ready} ->
                    case maps:get(<<"op">>, Request) of
                        <<"trade_item">> -> {{ok, (league_context(Leagues))#{
                            <<"item">> => maps:remove(prediction_item, Item),
                            <<"auto_search_allowed">> => retinue_poe2_trade_http:automatic_allowed(
                                maps:get(limits, Ready), now(Ready)),
                            <<"prediction_available">> => maps:get(prediction_item, Item) =/= undefined,
                            <<"search_options">> => Options}}, Ready};
                        <<"trade_search">> ->
                            case retinue_poe2_trade_query:build(Request, Item, Leagues, Options) of
                                {ok, Query} -> search(maps:get(<<"league">>, Request), Query, Ready);
                                Error -> {Error, Ready}
                            end
                    end;
                {error, Error, Failed} -> {{error, Error}, Failed}
            end;
        {error, Error, Next} -> {{error, Error}, Next}
    end.

league_context(Leagues) ->
    Default = case Leagues of
        [#{<<"id">> := Id} | _] -> Id;
        [] -> <<>>
    end,
    #{<<"leagues">> => Leagues, <<"league">> => Default}.

prepare(#{<<"text">> := Text}, State) ->
    case metadata(leagues, State) of
        {ok, Leagues, State1} ->
            case metadata(stats, State1) of
                {ok, Catalog, State2} ->
                    {ok, Item} = retinue_poe2_trade_item:parse(Text, Catalog),
                    case resolve_base(Item, State2) of
                        {ok, Resolved, State3} ->
                            Enrich = maps:get(enrich, State3, fun retinue_poe2_catalog:enrich/1),
                            Payload = retinue_poe2_prediction_item:build(Resolved),
                            {ok, (Enrich(Resolved))#{prediction_item => Payload}, Leagues, State3};
                        Error -> Error
                    end;
                Error -> Error
            end;
        Error -> Error
    end.

resolve_base(Item = #{<<"rarity">> := <<"Magic">>, <<"base">> := Name}, State) ->
    case metadata(items, State) of
        {ok, Bases, Next} ->
            Matches = [B || B <- Bases, string:find(<<" ", Name/binary, " ">>,
                                                   <<" ", B/binary, " ">>) =/= nomatch],
            Sorted = lists:reverse(lists:sort([{byte_size(B), B} || B <- Matches])),
            case Sorted of
                [{Size, _}, {Size, _} | _] -> failure(ambiguous_item_base, Next);
                [{_, Base} | _] -> {ok, Item#{<<"base">> => Base}, Next};
                [] -> failure(unknown_item_base, Next)
            end;
        Error -> Error
    end;
resolve_base(Item = #{<<"rarity">> := Rarity, <<"base">> := Base}, State)
  when Rarity =:= <<"Currency">>; Rarity =:= <<"Quest">> ->
    case metadata(items, State) of
        {ok, Bases, Next} ->
            Resolved = case lists:member(Base, Bases) of
                true -> Item;
                false -> Item#{<<"trade_unavailable">> => <<"not_in_trade_catalog">>}
            end,
            {ok, Resolved, Next};
        Error -> Error
    end;
resolve_base(Item, State) -> {ok, Item, State}.

metadata(Name, State = #{cache := Cache}) ->
    Now = now(State),
    Old = maps:get(Name, Cache, #{}),
    case maps:get(expires, Old, 0) > Now of
        true -> {ok, maps:get(data, Old), State};
        false ->
            Path = <<"/api/trade2/data/", (atom_to_binary(Name))/binary>>,
            Headers = maps:get(headers, Old, #{}),
            case http(metadata, get, Path, retinue_poe2_trade_http:validators(Headers), <<>>, State) of
                {ok, 304, NewHeaders, _, Next} when is_map_key(data, Old) ->
                    store_metadata(Name, maps:get(data, Old), maps:merge(Headers, NewHeaders), Next);
                {ok, 200, NewHeaders, Body, Next} ->
                    case decode_metadata(Name, Body) of
                        {ok, Data} -> store_metadata(Name, Data, NewHeaders, Next);
                        {error, Reason} -> failure(Reason, Next)
                    end;
                {ok, _, _, _, Next} -> failure(invalid_metadata_response, Next);
                Error -> Error
            end
    end.

store_metadata(Name, Data, Headers, State = #{cache := Cache}) ->
    Fresh = retinue_poe2_trade_http:freshness(Headers, now(State)),
    NextCache = case maps:get(store, Fresh) of
        true -> Cache#{Name => Fresh#{data => Data, headers => Headers}};
        false -> maps:remove(Name, Cache)
    end,
    {ok, Data, State#{cache => NextCache}}.

decode_metadata(Name, Body) ->
    try metadata_data(Name, json:decode(Body)) catch _:_ -> {error, invalid_metadata_response} end.
metadata_data(stats, Data) -> retinue_poe2_trade_item:catalog(Data);
metadata_data(filters, Data) -> retinue_poe2_trade_query:options(Data);
metadata_data(leagues, #{<<"result">> := Rows}) ->
    Leagues = [maps:with([<<"id">>, <<"text">>], R) || R = #{<<"id">> := Id, <<"text">> := Text} <- Rows,
               is_binary(Id), byte_size(Id) > 0, byte_size(Id) =< 256, is_binary(Text),
               maps:get(<<"realm">>, R, <<"poe2">>) =:= <<"poe2">>],
    true = Leagues =/= [] andalso length(Leagues) =< 256,
    {ok, Leagues};
metadata_data(items, #{<<"result">> := Groups}) ->
    Bases = lists:usort([Type || G <- Groups, E <- maps:get(<<"entries">>, G),
                                Type <- [maps:get(<<"type">>, E, undefined)],
                                is_binary(Type), byte_size(Type) > 0, byte_size(Type) =< 256]),
    true = Bases =/= [],
    {ok, Bases}.

search(League, Query, State = #{results := Results}) ->
    Key = {League, Query},
    Now = now(State),
    case maps:get(Key, Results, undefined) of
        #{expires := Expiry, value := {done, Data}} when Expiry > Now -> {{ok, Data}, State};
        #{expires := Expiry, value := {pending, Data, Ids}} when Expiry > Now ->
            fetch(Key, Data, Ids, State);
        _ ->
            Path = <<"/api/trade2/search/poe2/", (uri_string:quote(League))/binary>>,
            case http(search, post, Path, [], iolist_to_binary(json:encode(Query)), State) of
                {ok, 200, _, Body, Next} ->
                    case decode(Body) of
                        {ok, Json} ->
                            case retinue_poe2_trade_query:search_result(Json, League) of
                                {ok, Data, Ids} ->
                                    fetch(Key, Data, Ids, cache_result(Key, {pending, Data, Ids}, 120, Next));
                                Error -> {Error, Next}
                            end;
                        Error -> {Error, Next}
                    end;
                {ok, _, _, _, Next} -> {retinue_poe2_trade_query:error(invalid_search_response), Next};
                {error, Error, Next} -> {{error, Error}, Next}
            end
    end.

fetch(Key, Data, [], State) -> {{ok, Data}, cache_result(Key, {done, Data}, 30, State)};
fetch(Key, Data, Ids, State) ->
    Joined = iolist_to_binary(lists:join(<<",">>, Ids)),
    Path = <<"/api/trade2/fetch/", Joined/binary, "?query=",
             (uri_string:quote(maps:get(<<"search_id">>, Data)))/binary, "&realm=poe2">>,
    case http(fetch, get, Path, [], <<>>, State) of
        {ok, 200, _, Body, Next} ->
            case decode(Body) of
                {ok, Json} ->
                    case retinue_poe2_trade_query:listings(Json) of
                        {ok, Rows} ->
                            Result = Data#{<<"listings">> => Rows},
                            {{ok, Result}, cache_result(Key, {done, Result}, 30, Next)};
                        Error -> {Error, Next}
                    end;
                Error -> {Error, Next}
            end;
        {ok, _, _, _, Next} -> {retinue_poe2_trade_query:error(invalid_fetch_response), Next};
        {error, Error, Next} -> {{error, Error}, Next}
    end.

cache_result(Key, Value, TTL, State = #{results := Results}) ->
    Now = now(State),
    Live = maps:filter(fun(_, #{expires := Expiry}) -> Expiry > Now end, Results),
    Bounded = case map_size(Live) >= 16 andalso not maps:is_key(Key, Live) of
        true -> [{_, Oldest} | _] = lists:sort([{maps:get(expires, V), K} || {K, V} <- maps:to_list(Live)]),
                maps:remove(Oldest, Live);
        false -> Live
    end,
    State#{results => Bounded#{Key => #{expires => Now + TTL, value => Value}}}.

http(Group, Method, Path, Headers, Body, State = #{limits := Limits, http := HTTP}) ->
    Now = now(State),
    case retinue_poe2_trade_http:cooldown(Group, Limits, Now) of
        0 ->
            Spent = retinue_poe2_trade_http:spend(Group, Limits, Now),
            case HTTP(Method, Path, Headers, Body) of
                {ok, Status, RawHeaders, Response} ->
                    Safe = retinue_poe2_trade_http:headers(RawHeaders),
                    Observed = retinue_poe2_trade_http:observe(Group, Status, Safe, Spent, now(State)),
                    Next = State#{limits => Observed},
                    case Status of
                        N when N =:= 200; N =:= 304 -> {ok, Status, Safe, Response, Next};
                        429 -> {error, element(2, retinue_poe2_trade_query:error(rate_limited,
                                   retinue_poe2_trade_http:cooldown(Group, Observed, now(State)))), Next};
                        N when N =:= 401; N =:= 403 -> http_failure(anonymous_trade_unavailable, Group, Next);
                        N when N >= 500 -> http_failure(upstream_unavailable, Group, Next);
                        _ -> failure(trade_request_rejected, Next)
                    end;
                {error, Reason} -> failure(Reason, State#{limits => Spent})
            end;
        Seconds -> {error, element(2, retinue_poe2_trade_query:error(rate_limited, Seconds)), State}
    end.

http_failure(Reason, Group, State = #{limits := Limits}) ->
    case retinue_poe2_trade_http:cooldown(Group, Limits, now(State)) of
        0 -> failure(Reason, State);
        Seconds -> {error, element(2, retinue_poe2_trade_query:error(Reason, Seconds)), State}
    end.

decode(Body) ->
    try {ok, json:decode(Body)} catch _:_ -> retinue_poe2_trade_query:error(invalid_trade_response) end.
failure(Reason, State) -> {error, element(2, retinue_poe2_trade_query:error(Reason)), State}.
now(#{clock := Clock}) -> Clock().

-module(retinue_poe2_prediction).
-behaviour(gen_server).
-export([start_link/0, start_link/1, submit/3, request/3, clock/0]).
-export([init/1, handle_call/3, handle_cast/2, handle_info/2, terminate/2, code_change/3]).

start_link() -> start_link(#{}).
start_link(Options) ->
    case maps:get(name, Options, ?MODULE) of
        undefined -> gen_server:start_link(?MODULE, Options, []);
        Name -> gen_server:start_link({local, Name}, ?MODULE, Options, [])
    end.
submit(From, League, Item) -> gen_server:cast(?MODULE, {predict, From, League, Item}).
request(Server, League, Item) -> gen_server:call(Server, {predict, League, Item}, 20000).
clock() -> erlang:monotonic_time(second).

init(Options) ->
    process_flag(trap_exit, true),
    {ok, #{http => maps:get(http, Options, fun retinue_poe2_trade_http:predict/1),
        clock => maps:get(clock, Options, fun ?MODULE:clock/0), cache => #{},
        unavailable => #{}, backoff => undefined, active => undefined}}.

handle_call({predict, League, Item}, From, State) -> start(From, League, Item, State);
handle_call(_, _, State) -> {reply, unavailable(invalid_prediction_request), State}.
handle_cast({predict, From, League, Item}, State) ->
    case start(From, League, Item, State) of
        {reply, Reply, Next} -> reply(From, Reply), {noreply, Next};
        Pending -> Pending
    end;
handle_cast(_, State) -> {noreply, State}.

start(_, _, undefined, State) -> {reply, unavailable(unsupported_item), State};
start(From, League, Item, State) ->
    Now = now(State),
    Key = crypto:hash(sha256, term_to_binary({League, Item}, [deterministic])),
    case maps:get(Key, maps:get(cache, State), undefined) of
        #{expires := Expiry, reply := Reply} when Expiry > Now -> {reply, Reply, State};
        _ ->
            case {maps:get(League, maps:get(unavailable, State), Now), maps:get(backoff, State)} of
                {Expiry, _} when Expiry > Now -> {reply, unavailable(model_unavailable), State};
                {_, Expiry} when is_integer(Expiry), Expiry > Now ->
                    {reply, unavailable(prediction_cooldown), State};
                _ -> start_worker(From, Key, League, Item, State)
            end
    end.

start_worker(From, Key, League, Item, State = #{active := undefined}) ->
    Parent = self(), Http = maps:get(http, State),
    {Pid, Ref} = spawn_opt(fun() ->
        Payload = iolist_to_binary(json:encode(#{<<"gameId">> => <<"poe2">>,
            <<"leagueId">> => League, <<"items">> => [Item]})),
        Parent ! {prediction_done, self(), decode(Http(Payload))}
    end, [link, monitor]),
    Timer = erlang:send_after(17000, self(), {prediction_timeout, Pid}),
    {noreply, State#{active => #{pid => Pid, ref => Ref, timer => Timer, key => Key,
        league => League, waiters => [From]}}};
start_worker(From, Key, _, _, State = #{active := #{key := Key, waiters := Waiters} = Active})
  when length(Waiters) < 16 ->
    {noreply, State#{active => Active#{waiters => [From | Waiters]}}};
start_worker(_, _, _, _, State) ->
    {reply, retinue_poe2_trade_query:error(prediction_busy, 1), State}.

handle_info({prediction_done, Pid, Result}, State = #{active := #{pid := Pid}}) ->
    finish(Result, State);
handle_info({prediction_timeout, Pid}, State = #{active := #{pid := Pid}}) ->
    exit(Pid, kill), finish({failure, upstream_timeout, 60}, State);
handle_info({'DOWN', Ref, process, _, _}, State = #{active := #{ref := Ref}}) ->
    finish({failure, upstream_unavailable, 60}, State);
handle_info(_, State) -> {noreply, State}.

finish(Result, State = #{active := Active}) ->
    #{ref := Ref, timer := Timer, key := Key, league := League, waiters := Waiters} = Active,
    erlang:cancel_timer(Timer), erlang:demonitor(Ref, [flush]),
    Now = now(State),
    {Reply, TTL, Next} = case Result of
        {ok, Estimate} -> {{ok, #{<<"estimate">> => Estimate}}, 600, State};
        {unavailable, Reason} -> {unavailable(Reason), 300, State};
        {failure, model_unavailable, _} ->
            incident(model_unavailable),
            Missing = maps:filter(fun(_, Expiry) -> Expiry > Now end, maps:get(unavailable, State)),
            {unavailable(model_unavailable), 900, State#{unavailable => Missing#{League => Now + 900}}};
        {failure, Reason, Retry} ->
            incident(Reason),
            {unavailable(Reason), Retry, State#{backoff => Now + Retry}}
    end,
    lists:foreach(fun(From) -> reply(From, Reply) end, Waiters),
    Live = maps:filter(fun(_, #{expires := Expiry}) -> Expiry > Now end, maps:get(cache, Next)),
    Cache = case map_size(Live) >= 128 of
        true -> {_, Oldest} = lists:min([{maps:get(expires, V), K} || {K, V} <- maps:to_list(Live)]),
                maps:remove(Oldest, Live);
        false -> Live
    end,
    {noreply, Next#{active => undefined, cache => Cache#{Key => #{reply => Reply, expires => Now + TTL}}}}.

decode({ok, 200, _, Body}) ->
    try
        #{<<"results">> := [Result]} = json:decode(Body),
        case maps:get(<<"error_code">>, Result, null) of
            Error when Error =/= null, Error =/= <<>>, Error =/= false, Error =/= 0 ->
                {unavailable, item_not_supported};
            _ ->
                Amount = maps:get(<<"predicted_price">>, Result),
                true = nonnegative(Amount),
                Base = #{<<"amount">> => Amount, <<"currency">> => <<"exalted">>},
                case {maps:get(<<"predicted_price_low">>, Result, null),
                      maps:get(<<"predicted_price_high">>, Result, null),
                      maps:get(<<"predicted_price_confidence">>, Result, null)} of
                    {null, null, _} -> {ok, Base};
                    {Low, High, Confidence} ->
                        true = nonnegative(Low) andalso nonnegative(High) andalso Low =< High,
                        true = nonnegative(Confidence) andalso Confidence =< 100,
                        {ok, Base#{<<"uncertainty">> => #{<<"low">> => Low,
                            <<"high">> => High, <<"confidence">> => Confidence}}}
                end
        end
    catch _:_ -> {failure, invalid_prediction_response, 60} end;
decode({ok, 404, _, _}) -> {failure, model_unavailable, 900};
decode({ok, Status, Headers, _}) ->
    Reason = case Status of 429 -> rate_limited; _ -> upstream_unavailable end,
    Retry = retinue_poe2_trade_http:retry_after(maps:get(<<"retry-after">>, Headers, <<>>),
        erlang:system_time(second)),
    {failure, Reason, max(60, min(3600, Retry))};
decode({error, Reason}) -> {failure, Reason, 60}.

nonnegative(N) -> retinue_poe2_trade_item:sane_number(N) andalso N >= 0.
unavailable(Reason) -> {ok, #{<<"estimate">> => null, <<"reason">> => atom_to_binary(Reason)}}.
now(#{clock := Clock}) -> Clock().
reply({local, Recipient, Id}, Reply) -> Recipient ! {poe2_trade_result, Id, Reply};
reply(From, Reply) -> gen_server:reply(From, Reply).
incident(Reason) -> logger:warning("PoE2 prediction unavailable: ~p", [Reason]).

terminate(_, #{active := #{pid := Pid}}) -> exit(Pid, shutdown), ok;
terminate(_, _) -> ok.
code_change(_, State, _) -> {ok, State}.

-module(retinue_poe2_handler).
-export([init/1, request/2, event/2, encode_reply/2, encode_event/2, decode_request/2]).
-import(retinue_connection, [send_message/2, send_error/3, send_ok/4, request_id/1, start_local_request/4, feature_enabled/2, daemon_identity/0]).

init(State) -> State#{poe2_refs => #{}}.

encode_reply(Dataset, Data) -> poe2_native:encode(Dataset, Data).
encode_event(Event, Data) -> poe2_native:encode_event(Event, Data).
decode_request(Type, Bytes) -> poe2_native:decode_request(Type, Bytes).

request(#{<<"op">> := <<"subscribe">>, <<"dataset">> := <<"session">>} = Request,
               State = #{game := <<"poe2">>}) ->
    Id = request_id(Request),
    Refs = maps:get(poe2_refs, State),
    case map_size(Refs) of
        0 ->
            {ok, Ref, Snapshot} = retinue_poe2_service:subscribe(self()),
            send_ok(maps:get(socket, State), Id, <<"session">>, Snapshot),
            {ok, State#{poe2_refs => #{Ref => Id}}};
        _ ->
            send_error(maps:get(socket, State), Id, already_subscribed),
            {ok, State}
    end;
request(#{<<"op">> := <<"unsubscribe">>, <<"subscription">> := Id} = Request,
               State = #{game := <<"poe2">>}) ->
    Refs = maps:get(poe2_refs, State),
    lists:foreach(fun retinue_poe2_service:unsubscribe/1,
                  [Ref || {Ref, Value} <- maps:to_list(Refs), Value =:= Id]),
    send_message(maps:get(socket, State), #{<<"id">> => request_id(Request), <<"ok">> => true}),
    {ok, State#{poe2_refs => maps:filter(fun(_, Value) -> Value =/= Id end, Refs)}};
request(#{<<"op">> := <<"publish">>} = Request,
               State = #{game := <<"poe2">>, client := Client}) when Client =/= <<"poe2companion">> ->
    send_error(maps:get(socket, State), request_id(Request), observer_required),
    {ok, State};
request(#{<<"op">> := <<"trade_predict">>} = Request, State = #{game := <<"poe2">>}) ->
    case feature_enabled(<<"trade.prediction">>, State) of
        true -> submit_trade(Request);
        false -> send_error(maps:get(socket, State), request_id(Request), feature_not_negotiated)
    end,
    {ok, State};
request(#{<<"op">> := <<"currency_asset">>, <<"currency">> := Currency} = Request,
        State = #{game := <<"poe2">>}) ->
    {ok, start_local_request(request_id(Request), <<"currency_asset">>,
                             fun() -> retinue_poe2_assets:resolve(Currency) end, State)};
request(#{<<"op">> := Op} = Request, State = #{game := <<"poe2">>})
  when Op =:= <<"trade_item">>; Op =:= <<"trade_search">>; Op =:= <<"trade_leagues">>;
       Op =:= <<"trade_fetch">>; Op =:= <<"trade_restore">> ->
    %% Trade waits must not consume the request permits used by native publishes.
    submit_trade(Request),
    {ok, State};
request(Request, State = #{game := <<"poe2">>}) ->
    Producer = #{connection => self(), version => maps:get(version, maps:get(client_info, State, #{}), null)},
    Dataset = case maps:get(<<"op">>, Request, undefined) of
        <<"publish">> -> <<"session_published">>;
        _ -> <<"session">>
    end,
    {ok, start_local_request(request_id(Request), Dataset,
                             fun() -> retinue_poe2_service:request(Request, Producer) end, State)}.

submit_trade(#{<<"op">> := Op} = Request) ->
    %% Both trade and prediction preserve this opaque token through completion.
    retinue_poe2_trade:submit(self(), {poe2_native, request_id(Request), Op}, Request).

event({poe2_trade_result, {poe2_native, Id, Dataset}, {ok, Data}}, State) ->
    send_ok(maps:get(socket, State), Id, Dataset, Data),
    State;
event({poe2_trade_result, {poe2_native, Id, _Dataset}, {error, Error}}, State) ->
    send_message(maps:get(socket, State), #{<<"id">> => Id, <<"ok">> => false, <<"error">> => Error}),
    State;
event({retinue_poe2_session, Ref, Snapshot}, State) ->
            case maps:find(Ref, maps:get(poe2_refs, State)) of
                {ok, Id} -> send_message(maps:get(socket, State),
                    #{<<"event">> => <<"session">>, <<"game">> => <<"poe2">>,
                      <<"subscription">> => Id, <<"dataset">> => <<"session">>, <<"data">> => Snapshot});
                error -> ok
            end,
            State;
event(_Message, State) -> State.

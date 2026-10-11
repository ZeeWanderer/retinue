-module(retinue_poe2_handler).
-export([init/1, request/2, event/2]).
-import(retinue_connection, [send_json/2, send_error/3, send_ok/4, request_id/1, start_local_request/4, feature_enabled/2, daemon_identity/0]).

init(State) -> State#{poe2_refs => #{}}.

request(#{<<"op">> := <<"subscribe">>, <<"dataset">> := <<"session">>} = Request,
               State = #{game := <<"poe2">>}) ->
    Id = request_id(Request),
    Refs = maps:get(poe2_refs, State),
    case map_size(Refs) of
        0 ->
            {ok, Ref, Snapshot} = retinue_poe2_service:subscribe(self()),
            send_json(maps:get(socket, State), #{<<"id">> => Id, <<"ok">> => true,
                                                <<"data">> => Snapshot}),
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
    send_json(maps:get(socket, State), #{<<"id">> => request_id(Request), <<"ok">> => true}),
    {ok, State#{poe2_refs => maps:filter(fun(_, Value) -> Value =/= Id end, Refs)}};
request(#{<<"op">> := <<"publish">>} = Request,
               State = #{game := <<"poe2">>, client := Client}) when Client =/= <<"poe2companion">> ->
    send_error(maps:get(socket, State), request_id(Request), observer_required),
    {ok, State};
request(Request, State = #{game := <<"poe2">>}) ->
    {ok, start_local_request(request_id(Request), <<"session">>,
                             fun() -> retinue_poe2_service:request(Request) end, State)}.
event({retinue_poe2_session, Ref, Snapshot}, State) ->
            case maps:find(Ref, maps:get(poe2_refs, State)) of
                {ok, Id} -> send_json(maps:get(socket, State),
                    #{<<"event">> => <<"session">>, <<"game">> => <<"poe2">>,
                      <<"subscription">> => Id, <<"data">> => Snapshot});
                error -> ok
            end,
            State;
event(_Message, State) -> State.

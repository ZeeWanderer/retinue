-module(retinue_games).
-export([request/2, submit/3, subscribe/3, unsubscribe/2, owner/1, status/0, paths/1]).

request(warframe, Request) -> wf_rpc:request(Request);
request(poe2, status) ->
    Clients = maps:get(clients, retinue_local_api:status(), []),
    Observers = [Client || Client = #{game := <<"poe2">>, client := <<"poe2companion">>} <- Clients],
    (retinue_poe2_service:status())#{<<"observers">> => Observers};
request(poe2, events) -> retinue_poe2_service:events();
request(arc, status) -> #{observation => not_implemented};
request(Game, Request) -> {error, {unsupported_game_request, Game, Request}}.

submit(warframe, Client, Request) -> wf_rpc:submit(Client, Request);
submit(Game, _Client, _Request) -> {error, {unsupported_game, Game}}.

subscribe(warframe, Client, Request) -> wf_rpc:subscribe(Client, Request);
subscribe(Game, _Client, _Request) -> {error, {unsupported_game, Game}}.

unsubscribe(warframe, Ref) -> wf_rpc:unsubscribe(Ref);
unsubscribe(Game, _Ref) -> {error, {unsupported_game, Game}}.

owner(#{game := warframe} = Request) -> wf_protocol:owner(Request);
owner(_) -> undefined.

status() -> #{warframe => wf_rpc:status(), poe2 => poe2_health(),
              arc => #{observation => not_implemented}}.

poe2_health() -> try retinue_poe2_service:health() catch _:_ -> unavailable end.

paths(Games) ->
    [{state, retinue_paths:root(state)}, {runtime, retinue_paths:root(runtime)},
     {cache, retinue_paths:game(cache, poe2)}] ++ wf_rpc:paths(maps:get(warframe, Games, #{})).

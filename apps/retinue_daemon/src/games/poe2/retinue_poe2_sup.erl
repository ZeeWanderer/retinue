-module(retinue_poe2_sup).
-behaviour(supervisor).
-export([start_link/0, init/1, child_specs/0]).

start_link() -> retinue_sup:start_subtree(?MODULE).
init([]) -> {ok, {#{strategy => one_for_one, intensity => 3, period => 10}, child_specs()}}.
child_specs() ->
    [#{id => retinue_poe2_service, start => {retinue_poe2_service, start_link, []},
       type => worker, restart => permanent, shutdown => 5000,
       modules => [retinue_poe2_service]}].

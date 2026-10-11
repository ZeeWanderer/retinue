-module(retinue_games_sup).
-behaviour(supervisor).
-export([start_link/0, init/1, child_specs/0]).

start_link() -> retinue_sup:start_subtree(?MODULE).
init([]) -> {ok, {#{strategy => one_for_one, intensity => 3, period => 10}, child_specs()}}.
child_specs() -> [subtree(wfcli_warframe_sup), subtree(retinue_poe2_sup)].

subtree(Module) ->
    #{id => Module, start => {Module, start_link, []}, restart => permanent,
      shutdown => infinity, type => supervisor, modules => [Module]}.

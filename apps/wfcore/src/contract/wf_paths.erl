-module(wf_paths).

-export([cache_dir/0, cache_file/1, config_dir/0, config_file/1,
         data_dir/0, data_file/1, state_dir/0, state_file/1,
         runtime_dir/0, runtime_file/1, directories/1]).

cache_dir() -> retinue_paths:game(cache, warframe).
cache_file(Name) -> filename:join(cache_dir(), Name).
config_dir() -> retinue_paths:game(config, warframe).
config_file(Name) -> filename:join(config_dir(), Name).
data_dir() -> retinue_paths:game(data, warframe).
data_file(Name) -> filename:join(data_dir(), Name).
state_dir() -> retinue_paths:game(state, warframe).
state_file(Name) -> filename:join(state_dir(), Name).
runtime_dir() -> retinue_paths:root(runtime).
runtime_file(Name) -> filename:join(runtime_dir(), Name).

directories(wfcli) ->
    [{config, config_dir()}, {data, data_dir()},
     {cache, cache_dir()}, {state, state_dir()}];
directories(retinue_daemon) ->
    [{state, retinue_paths:root(state)}, {runtime, runtime_dir()},
     {cache, cache_dir()}, {assets, cache_file("assets")},
     {cache, retinue_paths:game(cache, poe2)},
     {data, data_dir()}, {state, state_dir()}];
directories(wfgui) ->
    Cache = cache_file("gui"),
    [{config, config_dir()}, {cache, Cache},
     {derivatives, filename:join([Cache, "derivatives", "v1"])},
     {runtime, runtime_dir()}].

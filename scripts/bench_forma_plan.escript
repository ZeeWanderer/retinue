#!/usr/bin/env escript
%%! -noshell
%% Simple benchmark harness for forma-plan planning performance.

-mode(compile).

main(Args) ->
    Opts = parse_args(Args, #{config => undefined, runs => 20, warmup => 5}),
    ConfigFile = maps:get(config, Opts),
    case ConfigFile of
        undefined ->
            io:format("usage: scripts/bench_forma_plan.escript --config FILE [--runs N] [--warmup N]~n"),
            halt(1);
        _ ->
            ok
    end,
    add_build_paths("_build/default/lib"),
    _ = application:ensure_all_started(retinue_cli),
    {ok, [Raw]} = wf_forma_config:load_files([ConfigFile]),
    {ok, Config} = wf_forma_model:normalize_config(Raw),
    Warmup = maps:get(warmup, Opts),
    Runs = maps:get(runs, Opts),
    run_warmup(Config, Warmup),
    Times = run_bench(Config, Runs),
    summarize(Times, Runs).

parse_args([], Acc) ->
    Acc;
parse_args(["--config", File | Rest], Acc) ->
    parse_args(Rest, Acc#{config := File});
parse_args(["--runs", N | Rest], Acc) ->
    parse_args(Rest, Acc#{runs := to_int(N, 20)});
parse_args(["--warmup", N | Rest], Acc) ->
    parse_args(Rest, Acc#{warmup := to_int(N, 5)});
parse_args([_ | Rest], Acc) ->
    parse_args(Rest, Acc).

to_int(Str, Default) ->
    case string:to_integer(Str) of
        {Int, _} -> Int;
        _ -> Default
    end.

add_build_paths(LibRoot) ->
    case file:list_dir(LibRoot) of
        {ok, Apps} ->
            lists:foreach(
              fun(App) ->
                  Ebin = filename:join([LibRoot, App, "ebin"]),
                  case filelib:is_dir(Ebin) of
                      true -> code:add_patha(Ebin);
                      false -> ok
                  end
              end,
              Apps);
        _ -> ok
    end.

run_warmup(_Config, N) when N =< 0 -> ok;
run_warmup(Config, N) ->
    _ = wf_forma_planner:plan(Config, #{}),
    run_warmup(Config, N - 1).

run_bench(Config, Runs) ->
    lists:map(
      fun(_) ->
          {Time, _} = timer:tc(fun() -> wf_forma_planner:plan(Config, #{}) end),
          Time
      end,
      lists:seq(1, Runs)).

summarize(Times, Runs) ->
    Min = lists:min(Times),
    Max = lists:max(Times),
    Avg = lists:sum(Times) div max(1, Runs),
    io:format("runs: ~p~nmin_us: ~p~nmax_us: ~p~navg_us: ~p~n", [Runs, Min, Max, Avg]).

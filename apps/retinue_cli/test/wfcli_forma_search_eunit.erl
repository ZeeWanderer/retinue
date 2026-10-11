%%%-------------------------------------------------------------------
%% EUnit tests for Forma search worker lifecycle.
%%%-------------------------------------------------------------------
-module(wfcli_forma_search_eunit).

-include_lib("eunit/include/eunit.hrl").

parallel_workers_are_linked_to_planner_test() ->
    Workers = wfcli_forma_search:start_parallel_workers(self(), #{}, #{}, 2),
    try
        lists:foreach(
          fun(Worker) ->
              {links, Links} = process_info(Worker, links),
              ?assert(lists:member(self(), Links))
          end,
          Workers)
    after
        lists:foreach(fun(Worker) -> Worker ! stop end, Workers)
    end.

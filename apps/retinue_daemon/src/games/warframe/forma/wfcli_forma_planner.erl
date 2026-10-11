%%%-------------------------------------------------------------------
%% Stable public API for Forma planning.
%%%-------------------------------------------------------------------
-module(wfcli_forma_planner).

-export([plan/2, assignments_for_plan/2, validate_plan/3]).

-type config() :: map().
-type flags() :: map().
-type plan() :: map().
-type forma_cost() :: non_neg_integer().
-type slot_assignments() :: #{term() => [{term(), term()}]}.

-doc "Find the lowest-cost polarity plan that fits every build.".
-spec plan(config(), flags()) -> {plan(), forma_cost()} | {error, term()}.
plan(Config, Flags) ->
    wfcli_forma_search:plan(Config, Flags).

-doc "Check that a plan satisfies config capacity and planner flags.".
-spec validate_plan(plan(), config(), flags()) -> ok | {error, term()}.
validate_plan(Plan, Config, Flags) ->
    wfcli_forma_search:validate_plan(Plan, Config, Flags).

-doc "Map a valid plan to build/mod labels assigned to each slot.".
-spec assignments_for_plan(config(), plan()) ->
    {ok, slot_assignments()} | {error, term()}.
assignments_for_plan(Config, Plan) ->
    wfcli_forma_assignment:for_plan(Config, Plan).

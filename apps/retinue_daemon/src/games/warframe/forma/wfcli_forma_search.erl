%%%-------------------------------------------------------------------
%% Simple Forma planner: explores slot polarity assignments to fit builds.
%%%-------------------------------------------------------------------
-module(wfcli_forma_search).

-export([plan/2, validate_plan/3]).

-ifdef(TEST).
-export([start_parallel_workers/4]).
-endif.

-type config() :: map().
-type flags() :: map().
-type plan() :: map().
-type forma_cost() :: non_neg_integer().

-doc "Find the lowest-cost polarity plan that fits every build in a forma config.".
-spec plan(config(), flags()) -> {plan(), forma_cost()} | {error, term()}.
plan(Config, Flags) ->
    Ctx = build_ctx(Config, Flags),
    case search(Ctx) of
        {ok, Plan, Cost} ->
            case validate_plan(Plan, Config, Flags) of
                ok -> {Plan, Cost};
                {error, _} = Err -> Err
            end;
        {error, _} = Err -> Err
    end.

-doc "Check that a planned polarity map still satisfies the config and planner flags.".
-spec validate_plan(plan(), config(), flags()) -> ok | {error, term()}.
validate_plan(Plan, Config, Flags) ->
    case wfcli_forma_rules:validate(Plan, build_ctx(Config, Flags)) of
        {ok, _} -> ok;
        Other -> Other
    end.

%%--------------------------------------------------------------------
%% Context building
%%--------------------------------------------------------------------
-doc "Normalize planner inputs into one context map shared by search, validation, and tiebreaks.".
build_ctx(#{item := Item, builds := Builds, constraints := Constraints} = Config, Flags) ->
    AllowOmni = get_flag(allow_omni, Flags, Constraints),
    AllowUmbral = get_flag(allow_umbral_forma, Flags, Constraints),
    PreferOmni = get_flag(prefer_omni, Flags, Constraints),
    MaxForma = maps:get(max_forma, Flags, maps:get(max_forma, Constraints, undefined)),
    SlotCandidates = order_candidates(candidates(Item, Builds, AllowOmni, AllowUmbral)),
    #{
      config => Config,
      item => Item,
      builds => Builds,
      flags => #{allow_omni => AllowOmni, allow_umbral_forma => AllowUmbral, prefer_omni => PreferOmni,
                 max_forma => MaxForma},
      slot_candidates => SlotCandidates
     }.

get_flag(Key, Flags, Constraints) ->
    case maps:get(Key, Flags, undefined) of
        undefined -> maps:get(Key, Constraints, false);
        V -> V
    end.

%%--------------------------------------------------------------------
%% Candidate generation
%%--------------------------------------------------------------------
-doc "Compute per-slot polarity candidates from current polarities, build mods, and Forma rules.".
 candidates(Item, Builds, AllowOmni, AllowUmbral) ->
    Slots = maps:get(slots, Item, []),
    SlotCount = length(Slots),
    ModsBySlot = mods_by_slot(Builds),
    FlexPols = maps:get(normal, ModsBySlot, []),
    Locked = maps:get(swap_locked_slots, Item, []),
    Movable = [Pol || Slot := Pol <- wfcli_forma_rules:current_plan(Item),
                      (Pol =:= omni orelse Pol =:= umbral),
                      not lists:member(Slot, Locked)],
    Reusable = fun(Slot) -> case lists:member(Slot, Locked) of
                               true -> []; false -> Movable
                           end end,
    NormalCandidates =
        [ {Idx, slot_candidates(Idx, maps:get(Idx, ModsBySlot, []) ++ FlexPols ++ Reusable(Idx),
                               safe_nth(Idx, Slots, none),
                               AllowOmni, AllowUmbral)}
          || Idx <- lists:seq(1, SlotCount)],
    AuraCand = {aura, slot_candidates(aura, maps:get(aura, ModsBySlot, []) ++ Reusable(aura),
                                      maps:get(aura_slot, Item, none),
                                      AllowOmni, AllowUmbral)},
    ExilusCand = {exilus, slot_candidates(exilus, maps:get(exilus, ModsBySlot, []) ++ Reusable(exilus),
                                          maps:get(exilus_slot, Item, none),
                                          AllowOmni, AllowUmbral)},
    [AuraCand, ExilusCand | NormalCandidates].

mods_by_slot(Builds) ->
    lists:foldl(
      fun(#{mods := Mods}, Acc) ->
          lists:foldl(
            fun(#{slot := Slot, polarity := Polarity}, Acc1) ->
                TargetSlot = case Slot of
                                 undefined -> normal;
                                 none -> normal;
                                 Other -> Other
                             end,
                Pols = maps:get(TargetSlot, Acc1, []),
                maps:put(TargetSlot, [Polarity | Pols], Acc1)
            end,
            Acc,
            Mods)
      end,
      #{},
      Builds).

slot_candidates(_SlotId, Pols, CurrentPol, AllowOmni, AllowUmbral) ->
    BasePols = [CurrentPol, none | [wfcli_forma_model:normalize_polarity(P) || P <- Pols]],
    WithOmni = maybe_append(omni, BasePols, AllowOmni),
    WithUmbral = maybe_append(umbral, WithOmni, AllowUmbral),
    uniq_preserve([P || P <- WithUmbral, P =/= unknown]).

maybe_append(Value, List, true) -> List ++ [Value];
maybe_append(_Value, List, false) -> List.

uniq_preserve(List) ->
    uniq_preserve(List, []).

uniq_preserve([], Acc) ->
    Acc;
uniq_preserve([H | T], Acc) ->
    case lists:member(H, Acc) of
        true -> uniq_preserve(T, Acc);
        false -> uniq_preserve(T, Acc ++ [H])
    end.

-doc "Search low-branch slots first so DFS finds useful incumbents early.".
order_candidates(List) ->
    lists:sort(
      fun({SlotA, A}, {SlotB, B}) ->
          candidate_order_key(SlotA, A) < candidate_order_key(SlotB, B)
      end,
      List).

candidate_order_key(Slot, Cands) ->
    {length(Cands), slot_sort_rank(Slot)}.

slot_sort_rank(aura) -> 0;
slot_sort_rank(exilus) -> 1;
slot_sort_rank(Slot) when is_integer(Slot) -> Slot + 1;
slot_sort_rank(_) -> 999.

%%--------------------------------------------------------------------
%% Search
%%--------------------------------------------------------------------
-doc "Run bounded planner search with a warm incumbent and optional parallel frontier.".
search(Ctx) ->
    search(Ctx, #{certify => true, warn => true}).

search(Ctx, Opts) ->
    case seed_best_plan(Ctx) of
        {_Plan0, 0} = Best -> best_to_result(Best);
        Seed ->
            SlotCands = maps:get(slot_candidates, Ctx),
            Budget = initial_budget(SlotCands),
            {BestPlan, _BudgetLeft, Exhausted} =
                case should_parallel(SlotCands) of
                    true ->
                        case warm_seed_best(SlotCands, Ctx, Seed, Budget) of
                            {WarmBest, _WarmBudgetLeft, true} ->
                                {WarmBest, 0, false};
                            {WarmBest, WarmBudgetLeft, false} ->
                                parallel_search(SlotCands, Ctx, WarmBest, WarmBudgetLeft)
                        end;
                    false ->
                        {SlotsList, CandList} = lists:unzip(SlotCands),
                        search_slots(SlotsList, CandList, #{}, Ctx, Seed, Budget)
                end,
            result_from_search(BestPlan, Exhausted, Ctx, Opts)
    end.

best_to_result(undefined) ->
    {error, no_plan};
best_to_result({Plan, Cost}) ->
    {ok, Plan, Cost}.

result_from_search(undefined, true, _Ctx, _Opts) ->
    {error, search_budget_exhausted};
result_from_search(BestPlan, true, Ctx, Opts) ->
    case maps:get(certify, Opts, true) andalso cost_optimal(BestPlan, Ctx) of
        true ->
            best_to_result(BestPlan);
        false ->
            maybe_warn_budget_exhausted(Opts),
            best_to_result(BestPlan)
    end;
result_from_search(BestPlan, false, _Ctx, _Opts) ->
    best_to_result(BestPlan).

maybe_warn_budget_exhausted(#{warn := false}) ->
    ok;
maybe_warn_budget_exhausted(_Opts) ->
    logger:warning("Forma search budget exhausted; returned plan may be incomplete", []).

cost_optimal({_Plan, Cost}, _Ctx) when Cost =< 0 ->
    true;
cost_optimal({_Plan, Cost}, Ctx) ->
    Flags0 = maps:get(flags, Ctx),
    ProofCtx = Ctx#{flags := Flags0#{max_forma => Cost - 1}},
    case search(ProofCtx, #{certify => false, warn => false}) of
        {error, no_plan} -> true;
        _ -> false
    end;
cost_optimal(_Other, _Ctx) ->
    false.

initial_budget(SlotCands) ->
    %% Limit combinatorial explosion while keeping enough room for typical cases.
    SlotCount = length(SlotCands),
    MaxBudget = case SlotCount =< 10 of
        true -> 500000;
        false -> 8000
    end,
    MinBudget = case SlotCount =< 10 of
        true -> 50000;
        false -> 2500
    end,
    Budget = lists:foldl(fun({_Slot, Cands}, Acc) -> Acc * max(1, min(length(Cands), 4)) end, 1, SlotCands),
    min(MaxBudget, max(MinBudget, Budget)).

should_parallel([{_Slot, Cands} | _Rest]) when length(Cands) >= 2 ->
    true;
should_parallel([_Single | Rest]) ->
    should_parallel(Rest);
should_parallel([]) -> false.

-doc "Spend a small serial budget to find an incumbent before worker pruning starts.".
warm_seed_best(_SlotCands, _Ctx, Seed, Budget) when Budget =< 0 ->
    {Seed, Budget, false};
warm_seed_best(SlotCands, Ctx, Seed, Budget) ->
    WarmBudget = min(Budget, min(50000, max(1000, Budget div 10))),
    {SlotsList, CandList} = lists:unzip(SlotCands),
    {Best, Remaining, Exhausted} = search_slots(SlotsList, CandList, #{}, Ctx, Seed, WarmBudget),
    Used = WarmBudget - Remaining,
    {Best, max(0, Budget - Used), not Exhausted}.

-doc "Split the remaining search space into deterministic worker tasks and merge best plans.".
parallel_search(SlotCands, Ctx, Seed, Budget) ->
    Tasks = parallel_tasks(SlotCands, Ctx, Seed),
    run_parallel_tasks(Tasks, Ctx, Seed, Budget).

-doc "Create parallel tasks by repeatedly fixing candidates on the heaviest remaining branch.".
parallel_tasks(SlotCands, Ctx, Seed) ->
    Target = parallel_target(SlotCands),
    expand_parallel_tasks([{#{}, SlotCands}], Ctx, Seed, Target).

parallel_target(SlotCands) ->
    Schedulers = case erlang:system_info(schedulers_online) of
                     N when is_integer(N), N > 0 -> N;
                     _ -> 1
                 end,
    SearchSpace = task_weight(SlotCands),
    min(SearchSpace, max(2, min(256, Schedulers * 8))).

-doc "Keep splitting tasks until worker count is useful but still bounded.".
expand_parallel_tasks(Tasks, _Ctx, _Seed, Target) when length(Tasks) >= Target ->
    Tasks;
expand_parallel_tasks(Tasks, Ctx, Seed, Target) ->
    case take_heaviest_task(Tasks) of
        none ->
            Tasks;
        {Task, OtherTasks} ->
            Children = expand_task(Task, Ctx, Seed),
            expand_parallel_tasks(Children ++ OtherTasks, Ctx, Seed, Target)
    end.

take_heaviest_task(Tasks) ->
    take_heaviest_task(Tasks, none, []).

take_heaviest_task([], none, _Acc) ->
    none;
take_heaviest_task([], {BestTask, _BestWeight}, Acc) ->
    {BestTask, lists:reverse(Acc)};
take_heaviest_task([{_Plan, []} = Task | Rest], Best, Acc) ->
    take_heaviest_task(Rest, Best, [Task | Acc]);
take_heaviest_task([{_Plan, TaskRest} = Task | Rest], none, Acc) ->
    take_heaviest_task(Rest, {Task, task_weight(TaskRest)}, Acc);
take_heaviest_task([{_Plan, TaskRest} = Task | Rest], {BestTask, BestWeight}, Acc) ->
    Weight = task_weight(TaskRest),
    case Weight > BestWeight of
        true -> take_heaviest_task(Rest, {Task, Weight}, [BestTask | Acc]);
        false -> take_heaviest_task(Rest, {BestTask, BestWeight}, [Task | Acc])
    end.

expand_task({PlanAcc, []}, _Ctx, _Seed) ->
    [{PlanAcc, []}];
expand_task({PlanAcc, [{Slot, Cands} | Rest]}, Ctx, Seed) ->
    [ {Plan1, Rest}
      || Cand <- Cands,
         Plan1 <- [maps:put(Slot, Cand, PlanAcc)],
         not should_prune(Plan1, Ctx, Seed) ].

-doc "Run worker tasks; keep zero/one task cases serial to avoid process overhead.".
run_parallel_tasks([], _Ctx, Seed, Budget) ->
    {Seed, Budget, false};
run_parallel_tasks([{PlanAcc, Rest}], Ctx, Seed, Budget) ->
    {RestSlots, RestCands} = lists:unzip(Rest),
    search_slots(RestSlots, RestCands, PlanAcc, Ctx, Seed, Budget);
run_parallel_tasks(Tasks, Ctx, Seed, Budget) ->
    Budgets = split_task_budgets(Budget, Tasks),
    WorkItems = lists:zip3(lists:seq(1, length(Tasks)), Tasks, Budgets),
    WorkerCount = parallel_worker_count(length(WorkItems)),
    Parent = self(),
    Workers = start_parallel_workers(Parent, Ctx, Seed, WorkerCount),
    {InitialItems, Queue} = take_n(WorkerCount, WorkItems),
    lists:foreach(fun({Worker, WorkItem}) -> Worker ! {run, WorkItem} end, lists:zip(Workers, InitialItems)),
    case collect_parallel_work(length(WorkItems), Queue, #{}, Seed, Ctx, 0, false) of
        {Best, Left, true} when Left > 0 ->
            %% Static slices can starve a hard branch while other workers finish early.
            {Slots, Cands} = lists:unzip(maps:get(slot_candidates, Ctx)),
            search_slots(Slots, Cands, #{}, Ctx, Best, Left);
        Result -> Result
    end.

start_parallel_workers(Parent, Ctx, Seed, WorkerCount) ->
    [spawn_link(fun() -> parallel_worker(Parent, Ctx, Seed) end)
     || _ <- lists:seq(1, WorkerCount)].

parallel_worker_count(TaskCount) ->
    Schedulers = case erlang:system_info(schedulers_online) of
                     N when is_integer(N), N > 0 -> N;
                     _ -> 1
                 end,
    min(TaskCount, Schedulers).

-doc "Worker loop: search one task slice, send result, wait for more work or stop.".
parallel_worker(Parent, Ctx, Seed) ->
    receive
        {run, {Index, {PlanAcc, Rest}, CandBudget}} ->
            {RestSlots, RestCands} = lists:unzip(Rest),
            {Best, BudgetLeft, Exhausted} = search_slots(RestSlots, RestCands, PlanAcc, Ctx, Seed, CandBudget),
            Parent ! {parallel_done, self(), Index, Best, BudgetLeft, Exhausted},
            parallel_worker(Parent, Ctx, Seed);
        stop ->
            ok
    end.

-doc "Feed tasks to workers and collect results in task-index order for deterministic merges.".
collect_parallel_work(0, _Queue, ResultsMap, Seed, Ctx, BudgetLeft, Exhausted) ->
    Results = [Best || {_Index, Best} <- lists:sort(maps:to_list(ResultsMap))],
    {merge_parallel(Results, Seed, Ctx), BudgetLeft, Exhausted};
collect_parallel_work(Remaining, Queue, ResultsMap, Seed, Ctx, BudgetLeft, Exhausted) ->
    receive
        {parallel_done, Worker, Index, Best, TaskBudgetLeft, TaskExhausted} ->
            RestQueue =
                case Queue of
                    [Next | Tail] ->
                        Worker ! {run, Next},
                        Tail;
                    [] ->
                        Worker ! stop,
                        []
                end,
            collect_parallel_work(Remaining - 1, RestQueue, maps:put(Index, Best, ResultsMap),
                                  Seed, Ctx, BudgetLeft + TaskBudgetLeft, Exhausted orelse TaskExhausted)
    end.

take_n(N, List) ->
    take_n(N, List, []).

take_n(0, List, Acc) ->
    {lists:reverse(Acc), List};
take_n(_N, [], Acc) ->
    {lists:reverse(Acc), []};
take_n(N, [H | T], Acc) ->
    take_n(N - 1, T, [H | Acc]).

split_task_budgets(_Budget, []) ->
    [];
split_task_budgets(Budget, Tasks) ->
    Weights = [task_weight(Rest) || {_PlanAcc, Rest} <- Tasks],
    Total = max(1, lists:sum(Weights)),
    Budgets0 = [max(1, (Budget * Weight) div Total) || Weight <- Weights],
    distribute_budget(Budgets0, Budget - lists:sum(Budgets0)).

distribute_budget(Budgets, Extra) when Extra =< 0 ->
    Budgets;
distribute_budget([], _Extra) ->
    [];
distribute_budget(Budgets, Extra) ->
    distribute_budget(Budgets, Extra, []).

distribute_budget(Rest, 0, Acc) ->
    lists:reverse(Acc) ++ Rest;
distribute_budget([], Extra, Acc) ->
    distribute_budget(lists:reverse(Acc), Extra, []);
distribute_budget([Budget | Rest], Extra, Acc) ->
    distribute_budget(Rest, Extra - 1, [Budget + 1 | Acc]).

task_weight(SlotCands) ->
    lists:foldl(
      fun({_Slot, Cands}, Acc) ->
          min(1000000, Acc * max(1, length(Cands)))
      end,
      1,
      SlotCands).

-doc "Merge worker incumbents using the same cost and tiebreak rules as serial search.".
merge_parallel(Results, Seed, Ctx) ->
    lists:foldl(fun(Best, Acc) -> merge_best(Acc, Best, Ctx) end, Seed, Results).

merge_best(undefined, Best, _Ctx) -> Best;
merge_best(Best, undefined, _Ctx) -> Best;
merge_best({PlanA, CostA}, {PlanB, CostB}, Ctx) ->
    case better_plan(CostB, PlanB, {PlanA, CostA}, Ctx) of
        true -> {PlanB, CostB};
        false -> {PlanA, CostA}
    end.

seed_best_plan(Ctx) ->
    Item = maps:get(item, Ctx),
    CurrentPlan = wfcli_forma_rules:current_plan(Item),
    case wfcli_forma_rules:validate(CurrentPlan, Ctx) of
        {ok, Cost} -> {CurrentPlan, Cost};
        _ -> undefined
    end.

-doc "Depth-first assignment search over candidate slots; budget counts explored partial plans.".
search_slots([], [], PlanAcc, Ctx, Best, Budget) ->
    {evaluate_plan(PlanAcc, Ctx, Best), Budget, false};
search_slots(_, _, _PlanAcc, _Ctx, Best, Budget) when Budget =< 0 ->
    {Best, 0, true};
search_slots([Slot | RestSlots], [Cands | RestCands], PlanAcc, Ctx, Best, Budget) ->
    lists:foldl(
      fun(Cand, {BestAcc, BudgetAcc, ExhaustedAcc}) ->
          case BudgetAcc =< 0 of
              true -> {BestAcc, 0, true};
              false ->
                  Plan1 = maps:put(Slot, Cand, PlanAcc),
                  NewBudget = BudgetAcc - 1,
                  case should_prune(Plan1, Ctx, BestAcc) of
                      true -> {BestAcc, NewBudget, ExhaustedAcc};
                      false ->
                          {UpdatedBest, Remaining, Exhausted} = search_slots(RestSlots, RestCands, Plan1, Ctx, BestAcc, NewBudget),
                          {UpdatedBest, Remaining, ExhaustedAcc orelse Exhausted}
                  end
          end
      end,
      {Best, Budget, false},
      Cands).

-doc "Validate a complete candidate plan and keep it only if it improves cost or tiebreaks.".
evaluate_plan(Plan, Ctx, Best) ->
    case wfcli_forma_rules:validate(Plan, Ctx) of
        {ok, FormaCost} ->
            case better_plan(FormaCost, Plan, Best, Ctx) of
                true -> {Plan, FormaCost};
                false -> Best
            end;
        {error, _} ->
            Best
    end.

-doc "Compare candidate plans by max-forma, Forma cost, fewer changes, reuse, then stable order.".
better_plan(_Cost, _Plan, undefined, _Ctx) ->
    true;
better_plan(Cost, Plan, {BestPlan, BestCost}, Ctx) ->
    Max = maps:get(max_forma, maps:get(flags, Ctx), undefined),
    case Max of
        undefined -> cost_better(Cost, Plan, BestCost, BestPlan, Ctx);
        _ when Cost > Max -> false;
        _ when BestCost > Max -> Cost =< Max;
        _ -> cost_better(Cost, Plan, BestCost, BestPlan, Ctx)
    end.

cost_better(Cost, _Plan, BestCost, _BestPlan, _Ctx) when Cost < BestCost ->
    true;
cost_better(Cost, _Plan, BestCost, _BestPlan, _Ctx) when Cost > BestCost ->
    false;
cost_better(_Cost, Plan, _BestCost, BestPlan, Ctx) ->
    Item = maps:get(item, Ctx),
    Changes = wfcli_forma_rules:slot_change_count(Plan, Item),
    BestChanges = wfcli_forma_rules:slot_change_count(BestPlan, Item),
    case Changes < BestChanges of
        true -> true;
        false ->
            case Changes > BestChanges of
                true -> false;
                false ->
                    Reuse = wfcli_forma_rules:reuse_count(Plan, Item),
                    BestReuse = wfcli_forma_rules:reuse_count(BestPlan, Item),
                    case Reuse > BestReuse of
                        true -> true;
                        false ->
                            case Reuse < BestReuse of
                                true -> false;
                                false ->
                                    RemCost = wfcli_forma_rules:removal_cost(Plan, Item),
                                    BestRemCost = wfcli_forma_rules:removal_cost(BestPlan, Item),
                                    case RemCost < BestRemCost of
                                        true -> true;
                                        false ->
                                            case RemCost > BestRemCost of
                                                true -> false;
                                                false -> prefer_tiebreak(Plan, BestPlan, Ctx)
                                            end
                                    end
                            end
                    end
            end
    end.

prefer_tiebreak(Plan, BestPlan, Ctx = #{flags := #{prefer_omni := true}}) ->
    Omni = omni_count(Plan),
    BestOmni = omni_count(BestPlan),
    case Omni > BestOmni of
        true -> true;
        false ->
            case Omni < BestOmni of
                true -> false;
                false -> stable_plan_tiebreak(Plan, BestPlan, Ctx)
            end
    end;
prefer_tiebreak(Plan, BestPlan, Ctx) ->
    stable_plan_tiebreak(Plan, BestPlan, Ctx).

omni_count(Plan) ->
    length([ok || {_S, P} <- maps:to_list(Plan), P =:= omni]).

-doc "Final deterministic tiebreak: prefer earlier candidate ranks in stable slot order.".
stable_plan_tiebreak(Plan, BestPlan, Ctx) ->
    stable_plan_key(Plan, Ctx) < stable_plan_key(BestPlan, Ctx).

stable_plan_key(Plan, Ctx) ->
    [candidate_rank(Slot, maps:get(Slot, Plan, none), Ctx) || Slot <- stable_slots(Ctx)].

stable_slots(Ctx) ->
    Item = maps:get(item, Ctx),
    [aura, exilus | lists:seq(1, length(maps:get(slots, Item, [])))].

candidate_rank(Slot, Polarity, Ctx) ->
    case lists:keyfind(Slot, 1, maps:get(slot_candidates, Ctx)) of
        {_, Cands} -> rank_in_candidates(Polarity, Cands, 0);
        false -> 999
    end.

rank_in_candidates(_Polarity, [], Rank) ->
    Rank;
rank_in_candidates(Polarity, [Polarity | _Rest], Rank) ->
    Rank;
rank_in_candidates(Polarity, [_Other | Rest], Rank) ->
    rank_in_candidates(Polarity, Rest, Rank + 1).

-doc "Prune partial plans that exceed max forma or cannot beat the incumbent lower bound.".
should_prune(Plan, Ctx, Best) ->
    Flags = maps:get(flags, Ctx),
    Max = maps:get(max_forma, Flags, undefined),
    PartialCost = wfcli_forma_rules:cost(Plan, maps:get(item, Ctx), Flags),
    ProjectedCost = PartialCost,
    OverBudget = case Max of
                     M when is_integer(M), M >= 0 -> ProjectedCost > M;
                     _ -> false
                 end,
    BetterFound = case Best of
                      {_BestPlan, BestCost} -> ProjectedCost > BestCost;
                      _ -> false
                  end,
    PartialCost >= 99999 orelse OverBudget orelse BetterFound orelse
        not wfcli_forma_assignment:possibly_fits_all(
              Plan, maps:get(item, Ctx), maps:get(builds, Ctx)).


safe_nth(N, List, Default) when is_integer(N), N > 0 ->
    case lists:nthtail(N-1, List) of
        [H | _] -> H;
        [] -> Default;
        _ -> Default
    end;
safe_nth(_, _, Default) -> Default.

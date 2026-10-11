%%%-------------------------------------------------------------------
%% Forma plan validity, cost, and current-polarity accounting.
%%%-------------------------------------------------------------------
-module(wfcli_forma_rules).

-export([validate/2, cost/3, current_plan/1, current_polarity/2,
         slot_change_count/2, reuse_count/2, removal_cost/2, operations/2]).

-type plan() :: map().

-doc "Validate capacity and Forma budget for a concrete plan.".
-spec validate(plan(), map()) -> {ok, non_neg_integer()} | {error, term()}.
validate(Plan, #{item := Item, builds := Builds, flags := Flags}) ->
    Cost = cost(Plan, Item, Flags),
    case Cost >= 99999 of
        true -> {error, forma_not_allowed};
        false -> validate_capacity(Plan, Item, Builds, Flags, Cost)
    end.

validate_capacity(Plan, Item, Builds, Flags, Cost) ->
    case maps:get(max_forma, Flags, undefined) of
        Max when is_integer(Max), Max >= 0, Cost > Max ->
            {error, over_budget};
        _ ->
            case wfcli_forma_assignment:fits_all(Plan, Item, Builds) of
                true -> {ok, Cost};
                false -> {error, capacity}
            end
    end.

-doc "Return Forma cost for changing current item polarities to target plan.".
-spec cost(plan(), map(), map()) -> non_neg_integer().
cost(Plan, Item, Flags) ->
    {Fixed, Targets} = lists:partition(
                        fun({Slot, _}) -> swap_locked(Slot, Item) end,
                        maps:to_list(Plan)),
    {AppliedCost, _Remaining} = cost_from_targets(Targets, Flags, current_pool(Item)),
    Cost = AppliedCost + lists:sum([fixed_cost(Slot, Target, Item, Flags)
                                    || {Slot, Target} <- Fixed]),
    case Cost =:= 0 andalso not maps:get(swap_unlocked, Item, true)
                    andalso slot_change_count(Plan, Item) > 0 of
        true ->
            Costs = [wfcli_forma_model:forma_cost(P) || P <- current_pool(Item),
                                                        application_allowed(P, Flags)],
            case Costs of [] -> 99999; _ -> lists:min(Costs) end;
        false -> Cost
    end.

fixed_cost(Slot, Target, Item, Flags) ->
    {Cost, _} = cost_from_targets([{Slot, Target}], Flags,
                                  [current_polarity(Slot, Item)]),
    Cost.

swap_locked(Slot, Item) ->
    lists:member(Slot, maps:get(swap_locked_slots, Item, [])).

-doc "Return current item polarities as a complete plan map.".
-spec current_plan(map()) -> plan().
current_plan(Item) ->
    Slots = maps:get(slots, Item, []),
    Normal =
        maps:from_list(lists:zip(lists:seq(1, length(Slots)), Slots)),
    Normal#{aura => maps:get(aura_slot, Item, none),
            exilus => maps:get(exilus_slot, Item, none)}.

-doc "Return current polarity for one item slot.".
-spec current_polarity(term(), map()) -> term().
current_polarity(aura, Item) ->
    maps:get(aura_slot, Item, none);
current_polarity(exilus, Item) ->
    maps:get(exilus_slot, Item, none);
current_polarity(Index, Item) when is_integer(Index), Index > 0 ->
    safe_nth(Index, maps:get(slots, Item, []), none);
current_polarity(_, _) ->
    none.

-doc "Count plan slots whose target differs from current polarity.".
-spec slot_change_count(plan(), map()) -> non_neg_integer().
slot_change_count(Plan, Item) ->
    length(
      [changed || {Slot, Polarity} <- maps:to_list(Plan),
                  Polarity =/= current_polarity(Slot, Item)]).

-doc "Count target polarities reusable from existing slots.".
-spec reuse_count(plan(), map()) -> non_neg_integer().
reuse_count(Plan, Item) ->
    Targets = [Polarity || {_Slot, Polarity} <- maps:to_list(Plan),
                           Polarity =/= none],
    reuse_count(Targets, current_pool(Item), 0).

-doc "Count Forma needed to remove existing polarities targeted as none.".
-spec removal_cost([{term(), term()}] | plan(), map()) -> non_neg_integer().
removal_cost(Plan, Item) when is_map(Plan) ->
    removal_cost(maps:to_list(Plan), Item);
removal_cost(Targets, Item) ->
    Fixed = [Slot || {Slot, none} <- Targets, swap_locked(Slot, Item),
                      current_polarity(Slot, Item) =/= none],
    EmptyTargets = [Slot || {Slot, none} <- Targets, not swap_locked(Slot, Item)],
    EmptyCurrent = [P || P <- current_pool(Item), P =:= none],
    length(Fixed) + max(0, length(EmptyTargets) - length(EmptyCurrent)).

cost_from_targets([], _Flags, Current) ->
    {0, Current};
cost_from_targets([{_Slot, Target} | Rest], Flags, Current) ->
    case take_polarity(Target, Current, Flags) of
        {ok, Remaining} ->
            cost_from_targets(Rest, Flags, Remaining);
        {apply, Cost, Remaining} ->
            {RestCost, Final} = cost_from_targets(Rest, Flags, Remaining),
            {Cost + RestCost, Final};
        {error, Cost} ->
            {Cost, Current}
    end.

take_polarity(Target, Current, Flags) ->
    case consume(Target, Current) of
        {ok, Remaining} ->
            {ok, Remaining};
        error ->
            case application_allowed(Target, Flags) of
                true -> {apply, wfcli_forma_model:forma_cost(Target), Current};
                false -> {error, 99999}
            end
    end.

application_allowed(omni, Flags) -> maps:get(allow_omni, Flags, false);
application_allowed(umbral, Flags) -> maps:get(allow_umbral_forma, Flags, false);
application_allowed(_, _) -> true.

reuse_count([], _Current, Count) ->
    Count;
reuse_count([Target | Rest], Current, Count) ->
    case consume(Target, Current) of
        {ok, Remaining} -> reuse_count(Rest, Remaining, Count + 1);
        error -> reuse_count(Rest, Current, Count)
    end.

consume(_Polarity, []) ->
    error;
consume(Polarity, [Polarity | Rest]) ->
    {ok, Rest};
consume(Polarity, [Head | Rest]) ->
    case consume(Polarity, Rest) of
        {ok, Remaining} -> {ok, [Head | Remaining]};
        error -> error
    end.

current_pool(Item) ->
    [Polarity || {Slot, Polarity} <- maps:to_list(current_plan(Item)),
                  not swap_locked(Slot, Item)].

-spec operations(plan(), map()) -> [map()].
operations(Plan, Item) ->
    Current = current_plan(Item),
    Targets = lists:sort(maps:to_list(Plan)),
    %% Reuse polarities before overwriting any slot that could supply one.
    {Swapped, Swaps} = lists:foldl(
      fun({Slot, Target}, {State, Ops}) ->
          Before = maps:get(Slot, State),
          Donors = [Other || {Other, P} <- lists:sort(maps:to_list(State)),
                             P =:= Target, Other =/= Slot,
                             P =/= maps:get(Other, Plan, P),
                             not swap_locked(Other, Item)],
          case {Before =:= Target orelse swap_locked(Slot, Item), Donors} of
              {false, [Donor | _]} ->
                  Op = #{action => swap, slot => Slot, other_slot => Donor,
                         before => Before, polarity => Target},
                  {State#{Slot => Target, Donor => Before}, [Op | Ops]};
              _ -> {State, Ops}
          end
      end, {Current, []}, Targets),
    Applies = [#{action => polarize, slot => Slot, before => maps:get(Slot, Swapped),
                 polarity => Target, forma => forma_kind(Target)}
               || {Slot, Target} <- Targets, maps:get(Slot, Swapped) =/= Target],
    OrderedSwaps = lists:reverse(Swaps),
    case {maps:get(swap_unlocked, Item, true), OrderedSwaps, Applies} of
        {false, [_ | _], []} ->
            [{_, Slot, Pol} | _] = lists:sort([
                {wfcli_forma_model:forma_cost(P), S, P} || S := P <- Current,
                                                        not swap_locked(S, Item)]),
            [#{action => polarize, slot => Slot, before => Pol, polarity => Pol,
               forma => forma_kind(Pol)} | OrderedSwaps];
        {false, [_ | _], [First | Rest]} ->
            %% Polarize its original location first to unlock swapping.
            OriginalSlot = lists:foldl(fun(Swap, S) -> swapped_slot(S, Swap) end,
                                       maps:get(slot, First), lists:reverse(OrderedSwaps)),
            rebase_operations([First#{slot => OriginalSlot} | OrderedSwaps ++ Rest], Current);
        _ -> OrderedSwaps ++ Applies
    end.

swapped_slot(Slot, #{slot := Slot, other_slot := Other}) -> Other;
swapped_slot(Slot, #{slot := Other, other_slot := Slot}) -> Other;
swapped_slot(Slot, _) -> Slot.

rebase_operations(Ops, Current) ->
    {Rebased, _} = lists:mapfoldl(fun(Op = #{slot := S}, State) ->
        Before = maps:get(S, State),
        case Op of
            #{action := swap, other_slot := Other} ->
                P = maps:get(Other, State),
                {Op#{before => Before, polarity => P}, State#{S => P, Other => Before}};
            #{polarity := P} -> {Op#{before => Before}, State#{S => P}}
        end
    end, Current, Ops),
    Rebased.

forma_kind(omni) -> omni;
forma_kind(umbral) -> umbral;
forma_kind(_) -> standard.

safe_nth(N, List, Default) when is_integer(N), N > 0 ->
    case lists:nthtail(N - 1, List) of
        [Value | _] -> Value;
        [] -> Default;
        _ -> Default
    end;
safe_nth(_, _, Default) ->
    Default.

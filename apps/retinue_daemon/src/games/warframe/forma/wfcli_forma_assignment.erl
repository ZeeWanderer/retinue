%%%-------------------------------------------------------------------
%% Capacity checks and deterministic mod-to-slot assignment.
%%%-------------------------------------------------------------------
-module(wfcli_forma_assignment).

-export([fits_all/3, possibly_fits_all/3, for_plan/2, loadouts/2]).

-type config() :: map().
-type plan() :: map().
-type slot_assignments() :: #{term() => [{term(), term()}]}.

-doc "Return whether every build fits under a concrete polarity plan.".
-spec fits_all(plan(), map(), [map()]) -> boolean().
fits_all(Plan, Item, Builds) ->
    {NormalPols, AuraPol, ExilusPol} = plan_polarities(Plan, Item),
    BaseCap = wfcli_forma_model:apply_reactor(
        maps:get(capacity, Item, 0), maps:get(reactor, Item, false)),
    lists:all(
      fun(Build) ->
          case build_fits(Build, NormalPols, AuraPol, ExilusPol, BaseCap) of
              {ok, _Used, _Assignments} -> true;
              _ -> false
          end
      end,
      Builds).

possibly_fits_all(Plan, Item, Builds) ->
    BaseCap = wfcli_forma_model:apply_reactor(
                maps:get(capacity, Item, 0), maps:get(reactor, Item, false)),
    Slots = [maps:get(I, Plan, optimistic) || I <- lists:seq(1, length(maps:get(slots, Item, [])))],
    lists:all(fun(Build) ->
        case build_fits(Build, Slots, maps:get(aura, Plan, optimistic),
                        maps:get(exilus, Plan, optimistic), BaseCap) of
            {ok, _, _} -> true;
            _ -> false
        end
    end, Builds).

-doc "Map a valid plan to build/mod labels assigned to each slot.".
-spec for_plan(config(), plan()) -> {ok, slot_assignments()} | {error, term()}.
for_plan(#{item := Item, builds := Builds}, Plan) ->
    {NormalPols, AuraPol, ExilusPol} = plan_polarities(Plan, Item),
    BaseCap = wfcli_forma_model:apply_reactor(
        maps:get(capacity, Item, 0), maps:get(reactor, Item, false)),
    lists:foldl(
      fun(Build, {ok, MapAcc}) ->
              case build_fits(Build, NormalPols, AuraPol, ExilusPol, BaseCap) of
                  {ok, _Used, Assignments} ->
                      {ok, add_assignments(Assignments, MapAcc)};
                  {error, Reason} ->
                      {error, Reason}
              end;
         (_Build, {error, _} = Error) ->
              Error
      end,
      {ok, #{}},
      Builds);
for_plan(_, _) ->
    {error, invalid_config}.

-spec loadouts(config(), plan()) -> [map()].
loadouts(#{item := Item, builds := Builds}, Plan) ->
    {NormalPols, AuraPol, ExilusPol} = plan_polarities(Plan, Item),
    BaseCap = wfcli_forma_model:apply_reactor(
                maps:get(capacity, Item, 0), maps:get(reactor, Item, false)),
    [begin
         Mods = maps:get(mods, Build),
         Indexed = [Mod#{name => Index} || {Index, Mod} <- lists:enumerate(Mods)],
         {ok, Used, Assignments} = build_fits(Build#{mods => Indexed}, NormalPols,
                                             AuraPol, ExilusPol, BaseCap),
         AuraGain = lists:sum([wfcli_forma_model:aura_value(
                                 maps:get(polarity, M), AuraPol, maps:get(cost, M))
                               || M = #{slot := aura} <- Mods]),
         #{capacity => BaseCap + AuraGain, drain => Used,
           assignments => maps:from_list([{Index, Slot}
                                           || {Slot, {_, Index}} <- Assignments])}
     end || Build <- Builds].

plan_polarities(Plan, Item) ->
    ItemSlots = maps:get(slots, Item, []),
    AuraPol = maps:get(aura, Plan, maps:get(aura_slot, Item, none)),
    ExilusPol = maps:get(exilus, Plan, maps:get(exilus_slot, Item, none)),
    NormalPols =
        [maps:get(I, Plan, safe_nth(I, ItemSlots, none))
         || I <- lists:seq(1, length(ItemSlots))],
    {NormalPols, AuraPol, ExilusPol}.

build_fits(#{mods := Mods, name := BuildName} = Build, NormalPols, AuraPol, ExilusPol, BaseCap) ->
    AuraMods = [M || M = #{slot := aura} <- Mods],
    AuraGain = lists:sum(
        [aura_value(
             maps:get(polarity, M), AuraPol, maps:get(cost, M))
         || M <- AuraMods]),
    Cap = BaseCap + AuraGain,
    ExilusMods = [M || M = #{slot := exilus} <- Mods],
    ExilusCost = lists:sum(
        [slot_cost(
             maps:get(polarity, M), ExilusPol, maps:get(cost, M))
         || M <- ExilusMods]),
    NormalMods = [M || M <- Mods, allow_normal_slot(M)],
    NormalCostEntries = [normal_slot_costs(BuildName, M, NormalPols) || M <- NormalMods],
    case lists:any(fun(Entry) -> Entry =:= {error, invalid_slot} end, NormalCostEntries) of
        true ->
            {error, invalid_slot};
        false ->
            SlotCosts = [Entry#{elements => maps:get(elements, M, [])}
                          || {{ok, Entry}, M} <- lists:zip(NormalCostEntries, NormalMods)],
            case assign_build(prepare_entries(SlotCosts), length(NormalPols),
                              maps:get(elemental_orders, Build, [[]])) of
                {ok, UsedCost, Assignments} ->
                    TotalCost = UsedCost + ExilusCost,
                    case TotalCost =< Cap of
                        true ->
                            {ok, TotalCost,
                             append_special_assignments(
                                 BuildName, AuraMods, ExilusMods, Assignments)};
                        false ->
                            {error, capacity}
                    end;
                {error, Reason} ->
                    {error, Reason}
            end
    end.

allow_normal_slot(#{slot := Slot}) when is_integer(Slot), Slot > 0 -> true;
allow_normal_slot(#{slot := Slot}) when Slot =:= undefined; Slot =:= none; Slot =:= normal -> true;
allow_normal_slot(_) -> false.

normal_slot_costs(BuildName,
                  #{slot := Slot, polarity := Polarity, cost := Cost, name := ModName},
                  NormalPols)
  when is_integer(Slot), Slot > 0 ->
    case Slot > length(NormalPols) of
        true ->
            {error, invalid_slot};
        false ->
            SlotPolarity = safe_nth(Slot, NormalPols, none),
            {ok, #{label => {BuildName, ModName},
                   slot_costs =>
                       [{Slot, slot_cost(
                                   Polarity, SlotPolarity, Cost)}]}}
    end;
normal_slot_costs(BuildName, #{polarity := Polarity, cost := Cost, name := ModName},
                  NormalPols) ->
    SlotCosts =
        [{Index, slot_cost(Polarity, SlotPolarity, Cost)}
         || {Index, SlotPolarity} <-
                lists:zip(lists:seq(1, length(NormalPols)), NormalPols)],
    {ok, #{label => {BuildName, ModName}, slot_costs => SlotCosts}}.

%% Unassigned polarities may match any mod, giving a safe minimum drain.
slot_cost(Polarity, optimistic, Cost) -> wfcli_forma_model:mod_cost(Polarity, Polarity, Cost);
slot_cost(Polarity, Slot, Cost) -> wfcli_forma_model:mod_cost(Polarity, Slot, Cost).

aura_value(Polarity, optimistic, Cost) -> wfcli_forma_model:aura_value(Polarity, Polarity, Cost);
aura_value(Polarity, Slot, Cost) -> wfcli_forma_model:aura_value(Polarity, Slot, Cost).

prepare_entries(SlotCosts) ->
    Entries =
        [#{id => Id,
           label => maps:get(label, Entry),
           elements => maps:get(elements, Entry, []),
           slot_costs => sort_slot_costs(maps:get(slot_costs, Entry, []))}
         || {Id, Entry} <- lists:zip(lists:seq(1, length(SlotCosts)), SlotCosts)],
    lists:sort(fun entry_order/2, Entries).

entry_order(A, B) ->
    ACosts = maps:get(slot_costs, A, []),
    BCosts = maps:get(slot_costs, B, []),
    case length(ACosts) =/= length(BCosts) of
        true -> length(ACosts) < length(BCosts);
        false -> min_cost(ACosts) > min_cost(BCosts)
    end.

min_cost([]) -> 0;
min_cost(SlotCosts) ->
    lists:min([Cost || {_Slot, Cost} <- SlotCosts]).

sort_slot_costs(SlotCosts) ->
    lists:sort(fun({_, CostA}, {_, CostB}) -> CostA =< CostB end, SlotCosts).

assign_build(Entries, Count, [Order | _]) when length(Order) =< 2 ->
    assign_mods(Entries, lists:seq(1, Count));
assign_build(Entries, Count, Orders) ->
    {Result, _Memo} = assign_elements(Entries, lists:seq(1, Count), [], Orders, #{}),
    Result.

assign_elements([], _Slots, _Seen, _Orders, Memo) -> {{ok, 0, []}, Memo};
assign_elements(Entries, Slots, _Seen, _Orders, Memo) when length(Entries) > length(Slots) ->
    {{error, no_slot}, Memo};
assign_elements(Entries, [Slot | RestSlots] = Slots, Seen, Orders, Memo) ->
    Key = {entry_ids(Entries), Slots, Seen},
    case maps:find(Key, Memo) of
        {ok, Result} -> {Result, Memo};
        error ->
            Choices = [{Entry, Cost} || Entry <- Entries,
                         {S, Cost} <- maps:get(slot_costs, Entry), S =:= Slot],
            {Seed, Memo1} = case length(Slots) > length(Entries) of
                true -> assign_elements(Entries, RestSlots, Seen, Orders, Memo);
                false -> {{error, no_slot}, Memo}
            end,
            Best0 = case Seed of {ok, C, A} -> {C, A}; _ -> undefined end,
            {Best, Memo2} = lists:foldl(fun({Entry, Cost}, {BestAcc, MemoAcc}) ->
                NextSeen = wfcli_forma_elements:unique(Seen ++ maps:get(elements, Entry)),
                case lists:any(fun(O) -> lists:prefix(NextSeen, O) end, Orders) of
                    false -> {BestAcc, MemoAcc};
                    true ->
                        {Child, NextMemo} = assign_elements(lists:delete(Entry, Entries),
                            RestSlots, NextSeen, Orders, MemoAcc),
                        case Child of
                            {ok, ChildCost, Assignments} ->
                                Candidate = {Cost + ChildCost,
                                             [{Slot, maps:get(label, Entry)} | Assignments]},
                                {update_best(Candidate, BestAcc), NextMemo};
                            _ -> {BestAcc, NextMemo}
                        end
                end
            end, {Best0, Memo1}, Choices),
            Result = case Best of
                undefined -> {error, no_slot};
                {Cost, Assignments} -> {ok, Cost, Assignments}
            end,
            {Result, Memo2#{Key => Result}}
    end.

assign_mods(Entries, SlotsAvail) ->
    {Result, _Memo} = assign_mods(Entries, SlotsAvail, #{}),
    Result.

assign_mods([], _SlotsAvail, Memo) ->
    {{ok, 0, []}, Memo};
assign_mods(Entries, SlotsAvail, Memo) ->
    Key = {entry_ids(Entries), SlotsAvail},
    case maps:get(Key, Memo, undefined) of
        undefined ->
            {Result, Memo1} = assign_mods_uncached(Entries, SlotsAvail, Memo),
            {Result, maps:put(Key, Result, Memo1)};
        Cached ->
            {Cached, Memo}
    end.

assign_mods_uncached([Entry | Rest], SlotsAvail, Memo) ->
    SlotCosts = maps:get(slot_costs, Entry, []),
    Label = maps:get(label, Entry),
    Allowed = [SlotCost || SlotCost = {Slot, _} <- SlotCosts,
                           lists:member(Slot, SlotsAvail)],
    case Allowed of
        [] ->
            {{error, no_slot}, Memo};
        _ ->
            {Best, MemoOut} =
                lists:foldl(
                  fun({Slot, Cost}, {BestAcc, MemoAcc}) ->
                      {Child, MemoNext} =
                          assign_mods(Rest, lists:delete(Slot, SlotsAvail), MemoAcc),
                      Next =
                          case Child of
                              {ok, ChildCost, Assignments} ->
                                  update_best(
                                      {ChildCost + Cost,
                                       [{Slot, Label} | Assignments]},
                                      BestAcc);
                              {error, _} ->
                                  BestAcc
                          end,
                      {Next, MemoNext}
                  end,
                  {undefined, Memo},
                  Allowed),
            case Best of
                undefined -> {{error, no_slot}, MemoOut};
                {BestCost, Assignments} ->
                    {{ok, BestCost, Assignments}, MemoOut}
            end
    end.

entry_ids(Entries) ->
    [maps:get(id, Entry, 0) || Entry <- Entries].

update_best({Cost, Assignments}, undefined) ->
    {Cost, Assignments};
update_best({Cost, _Assignments}, {BestCost, _} = Best) when Cost >= BestCost ->
    Best;
update_best({Cost, Assignments}, _Best) ->
    {Cost, Assignments}.

append_special_assignments(BuildName, AuraMods, ExilusMods, Assignments) ->
    Aura = [{aura, {BuildName, maps:get(name, Mod)}} || Mod <- AuraMods],
    Exilus = [{exilus, {BuildName, maps:get(name, Mod)}} || Mod <- ExilusMods],
    Assignments ++ Aura ++ Exilus.

add_assignments([], Map) ->
    Map;
add_assignments([{Slot, Label} | Rest], Map) ->
    Updated = maps:update_with(Slot, fun(List) -> [Label | List] end, [Label], Map),
    add_assignments(Rest, Updated).

safe_nth(N, List, Default) when is_integer(N), N > 0 ->
    case lists:nthtail(N - 1, List) of
        [Value | _] -> Value;
        [] -> Default;
        _ -> Default
    end;
safe_nth(_, _, Default) ->
    Default.

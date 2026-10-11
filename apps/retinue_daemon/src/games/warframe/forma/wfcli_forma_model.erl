%%%-------------------------------------------------------------------
%% Models for forma planning: capacities, slot polarities, weights.
%%%-------------------------------------------------------------------
-module(wfcli_forma_model).

-export([
    normalize_polarity/1,
    polarity_weight/1,
    forma_cost/1,
    base_capacity/2,
    apply_reactor/2,
    mod_cost/3,
    aura_value/3,
    normalize_config/1,
    polarity_symbol/1
]).

%% Kept as model API aliases; shared wire values live in `shared/src`.
normalize_polarity(Value) -> wfcli_polarity:normalize(Value).

polarity_symbol(Value) -> wfcli_polarity:symbol(Value).

%% Weight for Forma usage; Omni counts as 4, Umbral higher.
polarity_weight(omni) -> 4;
polarity_weight(umbral) -> 6;
polarity_weight(umbral_forma) -> 6;
polarity_weight(_Other) -> 1.

%% Forma cost for a slot target (omni/umbral vs standard).
forma_cost(Polarity) ->
    case Polarity of
        omni -> polarity_weight(omni);
        umbral -> polarity_weight(umbral);
        umbral_forma -> polarity_weight(umbral_forma);
        _ -> polarity_weight(standard)
    end.

%% Capacity semantics live with the shared polarity value contract.
mod_cost(ModPolarity, SlotPolarity, BaseCost) when BaseCost >= 0 ->
    wfcli_polarity:mod_cost(ModPolarity, SlotPolarity, BaseCost).

aura_value(ModPolarity, SlotPolarity, Cost) when Cost >= 0 ->
    wfcli_polarity:aura_value(ModPolarity, SlotPolarity, Cost).

%% Base capacity per item type at max rank (without reactor/catalyst).
base_capacity(warframe, _Opts) -> 30;
base_capacity(weapon, _Opts) -> 30;
base_capacity(melee, _Opts) -> 30;
base_capacity(companion, _Opts) -> 30;
base_capacity(_Unknown, _Opts) -> 0.

%% Apply reactor/catalyst doubling if present.
apply_reactor(Capacity, true) -> Capacity * 2;
apply_reactor(Capacity, false) -> Capacity;
apply_reactor(Capacity, _Other) -> Capacity.

normalize_config(#{file := File, item := Item, builds := Builds, constraints := Constraints}) ->
    {Item1, ItemErrs} = normalize_item(Item),
    {Builds1, BuildErrs} = normalize_builds(Builds),
    {Constraints1, ConstraintsErrs} = normalize_constraints(Constraints),
    Errors = ItemErrs ++ BuildErrs ++ ConstraintsErrs,
    case Errors of
        [] -> {ok, #{file => File, item => Item1, builds => Builds1, constraints => Constraints1}};
        _ -> {error, Errors}
    end.

normalize_item(Map) when is_map(Map) ->
    {Type, ErrType} = normalize_type(maps:get(<<"type">>, Map, undefined)),
    CapacityVal = maps:get(<<"capacity">>, Map, 0),
    {Capacity, ErrCap} = normalize_int(CapacityVal, "item.capacity", 0),
    Reactor = maps:get(<<"reactor">>, Map, false),
    AuraSlot = normalize_polarity(maps:get(<<"aura_slot">>, Map, undefined)),
    ExilusSlot = normalize_polarity(maps:get(<<"exilus_slot">>, Map, undefined)),
    Slots = normalize_slots(maps:get(<<"slots">>, Map, [])),
    {SwapLocked, SwapErrors} = normalize_swap_locks(
                                maps:get(<<"swap_locked_slots">>, Map, []), length(Slots)),
    Errs = ErrType ++ ErrCap ++ SwapErrors ++ require_field(Capacity, "item.capacity"),
    {#{type => Type, capacity => Capacity, reactor => Reactor,
       aura_slot => AuraSlot, exilus_slot => ExilusSlot, slots => Slots,
       swap_unlocked => maps:get(<<"swap_unlocked">>, Map, true) =:= true,
       swap_locked_slots => SwapLocked}, Errs};
normalize_item(_) ->
    {#{}, ["item must be a map"]}.

normalize_type(undefined) -> {undefined, ["item.type missing"]};
normalize_type(Bin) when is_binary(Bin) ->
    normalize_type(binary_to_list(Bin));
normalize_type(Str) when is_list(Str) ->
    case string:lowercase(Str) of
        "warframe" -> {warframe, []};
        "weapon" -> {weapon, []};
        "melee" -> {melee, []};
        "companion" -> {companion, []};
        "necramech" -> {necramech, []};
        Other -> {undefined, [io_lib:format("item.type unsupported: ~s", [Other])]}
    end;
normalize_type(Atom) when is_atom(Atom) ->
    normalize_type(atom_to_list(Atom));
normalize_type(_) ->
    {undefined, ["item.type invalid"]}.

normalize_slots(Slots) when is_list(Slots) ->
    [normalize_polarity(S) || S <- Slots];
normalize_slots(_) -> [].

normalize_swap_locks(Values, Count) when is_list(Values) ->
    Parsed = [normalize_slot(Value) || Value <- Values],
    Slots = [Slot || {Slot, []} <- Parsed,
                    Slot =:= aura orelse Slot =:= exilus orelse
                        (is_integer(Slot) andalso Slot > 0 andalso Slot =< Count)],
    case length(Slots) =:= length(Values) of
        true -> {lists:usort(Slots), []};
        false -> {[], ["item.swap_locked_slots contains an invalid slot"]}
    end;
normalize_swap_locks(_, _) ->
    {[], ["item.swap_locked_slots must be a list"]}.

normalize_builds(Builds) when is_list(Builds) ->
    {Reversed, Errs} =
        lists:foldl(
          fun(Build, {Acc, ErrAcc}) ->
              case normalize_build(Build) of
                  {Build1, []} -> {[Build1 | Acc], ErrAcc};
                  {Build1, ErrList} -> {[Build1 | Acc], ErrList ++ ErrAcc}
              end
          end,
          {[], []},
          Builds),
    {lists:reverse(Reversed), lists:reverse(Errs)};
normalize_builds(_) ->
    {[], ["builds must be a list"]}.

normalize_build(Map) when is_map(Map) ->
    Name = maps:get(<<"name">>, Map, maps:get(name, Map, undefined)),
    Mods = maps:get(<<"mods">>, Map, maps:get(mods, Map, [])),
    Arcanes = maps:get(<<"arcanes">>, Map, maps:get(arcanes, Map, [])),
    {Mods1, ModErrs} = normalize_mods(Mods),
    {Arcanes1, ArcErrs} = normalize_arcanes(Arcanes),
    {Elements, ElementErrors} = normalize_elements(elemental_order, Map),
    AbilityOverride = maps:get(<<"ability_override">>, Map,
                               maps:get(ability_override, Map, [])),
    Errs = require_field(Name, "build.name") ++ ModErrs ++ ArcErrs ++ ElementErrors,
    {#{name => Name, mods => Mods1, arcanes => Arcanes1,
       elemental_orders => wfcli_forma_elements:equivalent_orders(Elements),
       ability_override => normalize_ability_override(AbilityOverride)}, Errs};
normalize_build(_) ->
    {#{}, ["build must be a map"]}.

normalize_ability_override(Values) when is_list(Values) ->
    [Value || Value <- Values, is_binary(Value) orelse is_list(Value)];
normalize_ability_override(undefined) -> [];
normalize_ability_override(null) -> [];
normalize_ability_override(_Value) -> [].

normalize_mods(Mods) when is_list(Mods) ->
    {Reversed, Errs} =
        lists:foldl(
          fun(Mod, {Acc, ErrAcc}) ->
              case normalize_mod(Mod) of
                  {Mod1, []} -> {[Mod1 | Acc], ErrAcc};
                  {Mod1, ErrList} -> {[Mod1 | Acc], ErrList ++ ErrAcc}
              end
          end,
          {[], []},
          Mods),
    {lists:reverse(Reversed), lists:reverse(Errs)};
normalize_mods(_) ->
    {[], ["mods must be a list"]}.

normalize_arcanes(Arcanes) when is_list(Arcanes) ->
    {Reversed, Errs} =
        lists:foldl(
          fun(Arcane, {Acc, ErrAcc}) ->
              case normalize_arcane(Arcane) of
                  {Arcane1, []} -> {[Arcane1 | Acc], ErrAcc};
                  {Arcane1, ErrList} -> {[Arcane1 | Acc], ErrList ++ ErrAcc}
              end
          end,
          {[], []},
          Arcanes),
    {lists:reverse(Reversed), lists:reverse(Errs)};
normalize_arcanes(undefined) ->
    {[], []};
normalize_arcanes(_) ->
    {[], ["arcanes must be a list"]}.

normalize_arcane(Arcane) when is_binary(Arcane); is_list(Arcane); is_atom(Arcane) ->
    Name = Arcane,
    Errs = require_field(Name, "arcane.name"),
    {#{name => Name}, Errs};
normalize_arcane(Map) when is_map(Map) ->
    Name = maps:get(<<"name">>, Map, maps:get(name, Map, undefined)),
    RankRaw = maps:get(<<"rank">>, Map, maps:get(rank, Map, undefined)),
    {Rank, RankErrs} = case RankRaw of
                           undefined -> {undefined, []};
                           null -> {undefined, []};
                           _ -> normalize_int(RankRaw, "arcane.rank", undefined)
                       end,
    Errs = require_field(Name, "arcane.name") ++ RankErrs,
    Arcane1 = case Rank of
                  undefined -> #{name => Name};
                  _ -> #{name => Name, rank => Rank}
              end,
    {Arcane1, Errs};
normalize_arcane(_) ->
    {#{}, ["arcane must be a string or map"]}.

normalize_mod(Map) when is_map(Map) ->
    Name = maps:get(<<"name">>, Map, maps:get(name, Map, undefined)),
    PolRaw = maps:get(<<"polarity">>, Map, maps:get(polarity, Map, undefined)),
    CostRaw = maps:get(<<"cost">>, Map, maps:get(cost, Map, undefined)),
    {LookupPol, LookupCost} = lookup_mod_defaults(Name, PolRaw, CostRaw),
    Pol = normalize_polarity(coalesce(PolRaw, LookupPol)),
    {Cost, ErrCost} = normalize_int(coalesce(CostRaw, LookupCost),
                                    "mod.cost", 0),
    {Slot, SlotErr} = normalize_slot(maps:get(<<"slot">>, Map, maps:get(slot, Map, undefined))),
    {Elements, ElementErrors} = normalize_elements(elements, Map),
    %% Slot can be omitted (flex slot); keep errors only when an explicit slot is invalid.
    Errs = require_field(Name, "mod.name") ++ ErrCost ++ polarity_err(Pol) ++ SlotErr
           ++ ElementErrors,
    {#{name => Name, polarity => Pol, cost => Cost, slot => Slot,
       elements => Elements}, Errs};
normalize_mod(_) ->
    {#{}, ["mod must be a map"]}.

normalize_elements(Key, Map) ->
    case wfcli_forma_elements:normalize(
           maps:get(atom_to_binary(Key), Map, maps:get(Key, Map, []))) of
        {ok, Elements} -> {Elements, []};
        {error, _} -> {[], ["elements must contain only heat, cold, electricity or toxin"]}
    end.

lookup_mod_defaults(Name, PolRaw, CostRaw) ->
    case wfcli_forma_mod_db:lookup(Name) of
        {ok, #{polarity := Pol, cost := Cost}} ->
            maybe_warn_mod(Name, PolRaw, CostRaw, Pol, Cost),
            case needs_lookup(PolRaw, CostRaw) of
                true -> {Pol, Cost};
                false -> {undefined, undefined}
            end;
        _ -> {undefined, undefined}
    end.

needs_lookup(undefined, _) -> true;
needs_lookup(null, _) -> true;
needs_lookup(_, undefined) -> true;
needs_lookup(_, null) -> true;
needs_lookup(_, _) -> false.

coalesce(undefined, B) -> B;
coalesce(null, B) -> B;
coalesce(A, _) -> A.

maybe_warn_mod(Name, PolRaw, CostRaw, DbPol, DbCost) ->
    warn_polarity(Name, PolRaw, DbPol),
    warn_cost(Name, CostRaw, DbCost),
    ok.

warn_polarity(_Name, undefined, _DbPol) -> ok;
warn_polarity(_Name, null, _DbPol) -> ok;
warn_polarity(_Name, _PolRaw, undefined) -> ok;
warn_polarity(Name, PolRaw, DbPol) ->
    Pol = normalize_polarity(PolRaw),
    case {Pol, DbPol} of
        {unknown, _} -> ok;
        {P, P} -> ok;
        {none, _} -> ok;
        {_, none} -> ok;
        {_, _} ->
            logger:warning("mod ~s polarity ~p does not match ExportUpgrades (~p)",
                           [to_list(Name), Pol, DbPol])
    end.

warn_cost(_Name, undefined, _DbCost) -> ok;
warn_cost(_Name, null, _DbCost) -> ok;
warn_cost(_Name, _CostRaw, undefined) -> ok;
warn_cost(Name, CostRaw, DbCost) when is_integer(CostRaw), is_integer(DbCost) ->
    case CostRaw > DbCost of
        true ->
            logger:warning("mod ~s cost ~p exceeds ExportUpgrades max (~p)",
                           [to_list(Name), CostRaw, DbCost]);
        false -> ok
    end;
warn_cost(_, _, _) -> ok.

to_list(V) -> retinue_text:to_list(V).

normalize_constraints(Map) when is_map(Map) ->
    AllowOmni = maps:get(<<"allow_omni">>, Map, maps:get(allow_omni, Map, false)),
    AllowUmbral = maps:get(<<"allow_umbral_forma">>, Map, maps:get(allow_umbral_forma, Map, false)),
    PreferOmni = maps:get(<<"prefer_omni">>, Map, maps:get(prefer_omni, Map, false)),
    MaxRaw = maps:get(<<"max_forma">>, Map, maps:get(max_forma, Map, undefined)),
    {MaxForma, MaxErrs} = case MaxRaw of
        undefined -> {undefined, []};
        null -> {undefined, []};
        _ -> normalize_int(MaxRaw, "constraints.max_forma", undefined)
    end,
    Errs = case MaxForma of
        undefined -> MaxErrs;
        N when is_integer(N), N >= 0 -> MaxErrs;
        _ -> ["constraints.max_forma must be >= 0" | MaxErrs]
    end,
    {#{allow_omni => AllowOmni, allow_umbral_forma => AllowUmbral,
       prefer_omni => PreferOmni, max_forma => MaxForma}, Errs};
normalize_constraints(_) ->
    {#{}, ["constraints must be a map"]}.

require_field(undefined, Field) ->
    [io_lib:format("missing ~s", [Field])];
require_field(null, Field) ->
    [io_lib:format("missing ~s", [Field])];
require_field(_, _) -> [].

polarity_err(unknown) -> ["unknown polarity symbol"];
polarity_err(_) -> [].

normalize_slot(undefined) -> {undefined, []};
normalize_slot(null) -> {undefined, []};
normalize_slot(Bin) when is_binary(Bin) ->
    normalize_slot(binary_to_list(Bin));
normalize_slot("aura") -> {aura, []};
normalize_slot("stance") -> {aura, []};
normalize_slot("exilus") -> {exilus, []};
normalize_slot(Str) when is_list(Str) ->
    case string:to_integer(Str) of
        {Int, _} when Int > 0 -> {Int, []};
        _ -> {undefined, ["mod.slot must be >0 integer or aura/stance/exilus"]}
    end;
normalize_slot(Int) when is_integer(Int), Int > 0 -> {Int, []};
normalize_slot(_) -> {undefined, ["mod.slot must be >0 integer or aura/stance/exilus"]}.

normalize_int(undefined, Field, _Default) ->
    {undefined, require_field(undefined, Field)};
normalize_int(Value, _Field, _Default) when is_integer(Value) ->
    {Value, []};
normalize_int(Bin, Field, Default) when is_binary(Bin) ->
    normalize_int(binary_to_list(Bin), Field, Default);
normalize_int(Str, Field, Default) when is_list(Str) ->
    case string:to_integer(Str) of
        {Int, _} -> {Int, []};
        _ -> {Default, [io_lib:format("~s must be integer", [Field])]}
    end;
normalize_int(_, Field, Default) ->
    {Default, [io_lib:format("~s must be integer", [Field])]}.

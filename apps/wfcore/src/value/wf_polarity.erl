%%%-------------------------------------------------------------------
%% Canonical Forma polarity values shared by daemon data and CLI output.
%%%-------------------------------------------------------------------
-module(wf_polarity).

-export([normalize/1, symbol/1, compatibility/2, mod_cost/3, aura_value/3]).

-doc "Normalize a config or plan polarity to its protocol atom.".
-spec normalize(term()) -> madurai | vazarin | naramon | zenurik | unairu |
                           omni | umbral | umbral_forma | penjaga | none | unknown.
normalize(undefined) -> none;
normalize(null) -> none;
normalize(<<>>) -> none;
normalize(none) -> none;
normalize(unknown) -> unknown;
normalize(madurai) -> madurai;
normalize(vazarin) -> vazarin;
normalize(naramon) -> naramon;
normalize(zenurik) -> zenurik;
normalize(unairu) -> unairu;
normalize(omni) -> omni;
normalize(umbral) -> umbral;
normalize(umbral_forma) -> umbral_forma;
normalize(penjaga) -> penjaga;
normalize(0) -> none;
normalize(1) -> madurai;
normalize(2) -> vazarin;
normalize(3) -> naramon;
normalize(4) -> zenurik;
normalize(5) -> penjaga;
normalize(7) -> unairu;
normalize(8) -> umbral;
normalize(9) -> omni;
normalize(Bin) when is_binary(Bin) -> normalize(binary_to_list(Bin));
normalize(Str) when is_list(Str) ->
    case string:lowercase(Str) of
        "none" -> none;
        "unknown" -> unknown;
        "ap_universal" -> none;
        "universal" -> none;
        "ap_attack" -> madurai;
        "madurai" -> madurai;
        "ap_defense" -> vazarin;
        "vazarin" -> vazarin;
        "ap_tactic" -> naramon;
        "naramon" -> naramon;
        "ap_power" -> zenurik;
        "zenurik" -> zenurik;
        "ap_precept" -> penjaga;
        "penjaga" -> penjaga;
        "ap_ward" -> unairu;
        "ap_umbra" -> umbral;
        "umbra" -> umbral;
        "umbral" -> umbral;
        "umbral_forma" -> umbral_forma;
        "ap_any" -> omni;
        "any" -> omni;
        "omni" -> omni;
        "aura" -> omni;
        "v" -> madurai;
        "d" -> vazarin;
        "-" -> naramon;
        "bar" -> naramon;
        "=" -> zenurik;
        "flame" -> zenurik;
        "unairu" -> unairu;
        "y" -> unairu;
        "u" -> umbral;
        "o" -> omni;
        "p" -> penjaga;
        "" -> none;
        _ -> unknown
    end;
normalize(Atom) when is_atom(Atom) -> normalize(atom_to_list(Atom));
normalize(_) -> unknown.

-doc "Return the stable YAML and terminal symbol for a polarity atom.".
-spec symbol(term()) -> string() | null.
symbol(madurai) -> "V";
symbol(vazarin) -> "D";
symbol(naramon) -> "-";
symbol(zenurik) -> "=";
symbol(unairu) -> "Y";
symbol(umbral) -> "U";
symbol(omni) -> "O";
symbol(penjaga) -> "P";
symbol(none) -> null;
symbol(unknown) -> "unknown";
symbol(_) -> "unknown".

-doc "Classify how a mod polarity relates to an equipment slot polarity.".
-spec compatibility(term(), term()) -> matched | mismatched | neutral | unknown.
compatibility(ModValue, SlotValue) ->
    Mod = normalize(ModValue),
    Slot = normalize(SlotValue),
    case {Mod, Slot} of
        {unknown, _} -> unknown;
        {_, unknown} -> unknown;
        {none, _} -> neutral;
        {_, none} -> neutral;
        {umbral, omni} -> mismatched;
        {_, omni} -> matched;
        {Polarity, Polarity} -> matched;
        _ -> mismatched
    end.

-doc "Return installed capacity drain for a mod and slot polarity.".
-spec mod_cost(term(), term(), non_neg_integer()) -> non_neg_integer().
mod_cost(ModPolarity, SlotPolarity, Drain) when is_integer(Drain), Drain >= 0 ->
    case compatibility(ModPolarity, SlotPolarity) of
        matched -> (Drain + 1) div 2;
        mismatched -> ((Drain * 5) + 2) div 4;
        _ -> Drain
    end.

-doc "Return capacity contributed by an Aura or Stance mod.".
-spec aura_value(term(), term(), non_neg_integer()) -> non_neg_integer().
aura_value(ModPolarity, SlotPolarity, Value) when is_integer(Value), Value >= 0 ->
    case compatibility(ModPolarity, SlotPolarity) of
        matched -> Value * 2;
        mismatched -> (Value * 4) div 5;
        _ -> Value
    end.

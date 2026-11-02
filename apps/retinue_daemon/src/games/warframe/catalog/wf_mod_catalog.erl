%%%-------------------------------------------------------------------
%% Canonical mod metadata lookup shared by worldstate and Forma planning.
%%%-------------------------------------------------------------------
-module(wf_mod_catalog).

-export([details/1, lookup_by_name/1, card_text/2, enrich_card_text/1]).

-doc "Return canonical metadata for one mod unique name.".
-spec details(term()) -> map() | undefined.
details(undefined) -> undefined;
details(ItemType) ->
    maps:get(retinue_text:to_binary(ItemType), wf_resolve_registry:mod_map(), undefined).

-spec card_text(term(), map() | undefined) -> map().
card_text(Rank, Details) when is_map(Details) ->
    Base = case maps:get(compat, Details, undefined) of
        Name when is_binary(Name), byte_size(Name) > 0 -> #{<<"compat_name">> => Name};
        _ -> #{}
    end,
    case maps:get(level_stats, Details, undefined) of
        Levels when is_list(Levels), is_integer(Rank), Rank >= 0, length(Levels) > Rank ->
            case lists:nth(Rank + 1, Levels) of
                #{<<"stats">> := Stats} when is_list(Stats) -> Base#{<<"effects">> => Stats};
                _ -> Base
            end;
        _ -> Base
    end;
card_text(_Rank, _Details) -> #{}.

-spec enrich_card_text(map()) -> map().
enrich_card_text(Upgrade) ->
    Text = card_text(maps:get(<<"rank">>, Upgrade, undefined),
                    details(maps:get(<<"item_type">>, Upgrade, undefined))),
    maps:merge(Upgrade, Text).

-doc "Resolve a display name to the preferred Forma-planner polarity and cost.".
-spec lookup_by_name(term()) -> {ok, map()} | {error, no_name | not_found}.
lookup_by_name(Name) ->
    case normalize_name(Name) of
        undefined -> {error, no_name};
        Key ->
            case maps:get(Key, wf_resolve_registry:mod_name_index(), undefined) of
                undefined -> {error, not_found};
                Candidates -> {ok, select_candidate(Candidates)}
            end
    end.

normalize_name(undefined) -> undefined;
normalize_name(null) -> undefined;
normalize_name(Bin) when is_binary(Bin) -> string:lowercase(binary_to_list(Bin));
normalize_name(Str) when is_list(Str) -> string:lowercase(Str);
normalize_name(Atom) when is_atom(Atom) -> string:lowercase(atom_to_list(Atom));
normalize_name(_) -> undefined.

select_candidate(Candidates) ->
    Filtered = [Candidate || Candidate <- Candidates,
                             maps:get(exclude_from_codex, Candidate, false) =/= true],
    Use = case Filtered of [] -> Candidates; _ -> Filtered end,
    [First | _] = lists:sort(fun candidate_order/2, Use),
    #{polarity => maps:get(polarity, First, none),
      cost => maps:get(cost, First, undefined)}.

candidate_order(A, B) ->
    retinue_text:to_list(maps:get(unique, A, "")) =<
        retinue_text:to_list(maps:get(unique, B, "")).

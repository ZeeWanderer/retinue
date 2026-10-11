%%%-------------------------------------------------------------------
%% Shared mastery-rank calculations for player-facing daemon views.
%%%-------------------------------------------------------------------
-module(wfcli_player_mastery).

-export([progress/2, mastered/2, rank/3]).

-type progress() :: #{rank := non_neg_integer(),
                      max_rank := pos_integer(),
                      mastery_per_rank := 100 | 200}.

-doc "Calculate rank and mastery-point rate for one catalog item and XP value.".
-spec progress(map(), integer()) -> progress().
progress(Item, Xp) ->
    Category = maps:get(<<"category">>, Item, <<>>),
    Type = maps:get(<<"type">>, Item, <<>>),
    Name = maps:get(<<"name">>, Item, <<>>),
    MaxRank = max_rank(Name),
    PerRank = mastery_per_rank(Category, Type),
    Rank = rank(Xp, PerRank, MaxRank),
    #{rank => Rank, max_rank => MaxRank, mastery_per_rank => PerRank}.

-doc "Return whether an item's recorded XP reaches its maximum mastery rank.".
-spec mastered(map(), integer()) -> boolean().
mastered(Item, Xp) ->
    #{rank := Rank, max_rank := MaxRank} = progress(Item, Xp),
    Rank >= MaxRank.

-doc "Convert item XP to rank for a known mastery-point rate and rank cap.".
-spec rank(integer(), 100 | 200, pos_integer()) -> non_neg_integer().
rank(Xp, PerRank, MaxRank) ->
    Divisor = case PerRank of 200 -> 1000; 100 -> 500 end,
    min(MaxRank, trunc(math:sqrt(max(0, Xp) / Divisor))).

mastery_per_rank(Category, Type) when Category =:= <<"Warframes">>;
                                           Category =:= <<"Archwing">>;
                                           Category =:= <<"Pets">>;
                                           Category =:= <<"Sentinels">>;
                                           Type =:= <<"K-Drive Component">> -> 200;
mastery_per_rank(_Category, _Type) -> 100.

max_rank(Name) ->
    case lists:any(fun(Prefix) -> binary:match(Name, Prefix) =/= nomatch end,
                   [<<"Kuva ">>, <<"Tenet ">>, <<"Coda ">>, <<"Paracesis">>]) of
        true -> 40;
        false -> 30
    end.

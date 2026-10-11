%%%-------------------------------------------------------------------
%% Suggestion helpers for mistyped arguments.
%%%-------------------------------------------------------------------
-module(retinue_cli_suggest).

-export([suggest/2, suggest_match/2]).

suggest(Unknown0, Candidates) ->
    Unknown = normalize(Unknown0),
    case Unknown of
        "" -> "";
        _ ->
            case suggest_match(Unknown, Candidates) of
                {ok, Best} -> " (did you mean " ++ Best ++ "?)";
                none -> ""
            end
    end.

suggest_match(Unknown0, Candidates) ->
    Unknown = normalize(Unknown0),
    case Unknown of
        "" -> none;
        _ ->
            Scores = [{C, levenshtein(Unknown, normalize(C))} || C <- Candidates],
            {Best, Dist} = best_match(Scores),
            case Dist =< 3 of
                true -> {ok, Best};
                false -> none
            end
    end.

normalize(Str0) ->
    Str = retinue_text:to_list(Str0),
    Str1 = string:trim(Str),
    string:lowercase(strip_leading_dashes(Str1)).

strip_leading_dashes([$- | Rest]) -> strip_leading_dashes(Rest);
strip_leading_dashes(Rest) -> Rest.

levenshtein(A, B) ->
    Row0 = lists:seq(0, length(B)),
    RowN = lists:foldl(
      fun(CharA, PrevRow) ->
          {Row, _} = lists:foldl(
            fun({CharB, PrevRowVal}, {RowAcc, PrevDiag}) ->
                [Left | _] = RowAcc,
                Cost = case CharA =:= CharB of true -> 0; false -> 1 end,
                Val = lists:min([Left + 1, PrevRowVal + 1, PrevDiag + Cost]),
                {[Val | RowAcc], PrevRowVal}
            end,
            {[hd(PrevRow) + 1], hd(PrevRow)},
            lists:zip(B, tl(PrevRow))
          ),
          lists:reverse(Row)
      end,
      Row0,
      A),
    lists:last(RowN).

best_match([]) ->
    {"", 999};
best_match([First | Rest]) ->
    lists:foldl(
      fun({C, D}, {BestC, BestD}) ->
          case D < BestD of
              true -> {C, D};
              false -> {BestC, BestD}
          end
      end,
      First,
      Rest).

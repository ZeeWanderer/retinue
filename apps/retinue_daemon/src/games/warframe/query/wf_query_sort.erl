%%%-------------------------------------------------------------------
%% Shared query sort parsing helpers.
%%%-------------------------------------------------------------------
-module(wf_query_sort).

-export([parse/1, compare/3]).

parse(Value0) ->
    Val = retinue_text:to_list(Value0),
    {Dir0, Rest0} = case Val of
        [$- | RestTail] -> {desc, RestTail};
        [$+ | RestTail] -> {asc, RestTail};
        _ -> {asc, Val}
    end,
    {Field, Dir1} = case string:split(Rest0, ":", trailing) of
        [Head, Suffix] ->
            case string:lowercase(Suffix) of
                "desc" -> {Head, desc};
                "asc" -> {Head, asc};
                _ -> {Rest0, Dir0}
            end;
        _ -> {Rest0, Dir0}
    end,
    #{key => string:trim(Field), dir => Dir1}.

compare(asc, A, B) -> A =< B;
compare(desc, A, B) -> A >= B;
compare(_, A, B) -> A =< B.

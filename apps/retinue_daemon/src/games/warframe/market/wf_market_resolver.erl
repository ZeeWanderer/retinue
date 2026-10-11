%%%-------------------------------------------------------------------
%% Fuzzy Warframe Market item-name resolution for OCR labels.
%%%-------------------------------------------------------------------
-module(wf_market_resolver).

-export([resolve/3, describe/2]).

-doc "Resolve each label to ranked Market items using normalized edit distance.".
-spec resolve([binary()], [map()], 1..5) -> [map()].
resolve(Labels, Items, Limit) ->
    Candidates = [Candidate || Item <- Items,
                                Candidate <- [candidate(Item)],
                                Candidate =/= undefined],
    [#{label => Label, matches => matches(Label, Candidates, Limit)} || Label <- Labels].

-doc "Return exact Market metadata by item ID, slug, or English name.".
-spec describe([binary()], [map()]) -> {[map()], [binary()]}.
describe(Values, Items) ->
    Lookup = lists:foldl(fun add_lookup/2, #{}, Items),
    {Found0, Missing0} = lists:foldl(
      fun(Value, {Found, Missing}) ->
          Key = normalize_exact(Value),
          case maps:get(Key, Lookup, undefined) of
              undefined -> {Found, [Value | Missing]};
              Item -> {[descriptor(Item) | Found], Missing}
          end
      end,
      {[], []}, Values),
    {lists:reverse(Found0), lists:reverse(Missing0)}.

candidate(Item) ->
    Slug = maps:get(<<"slug">>, Item, <<>>),
    Name = item_name(Item),
    English = maps:get(<<"en">>, maps:get(<<"i18n">>, Item, #{}), #{}),
    Normalized = normalize(Name),
    case {Slug, Normalized} of
        {<<>>, _} -> undefined;
        {_, []} -> undefined;
        _ -> #{name => Name, slug => Slug, normalized => Normalized,
               ducats => maps:get(<<"ducats">>, Item, undefined),
               game_ref => maps:get(<<"gameRef">>, Item, undefined),
               icon => maps:get(<<"icon">>, English, undefined),
               thumb => maps:get(<<"thumb">>, English, undefined),
               sub_icon => maps:get(<<"subIcon">>, English, undefined)}
    end.

add_lookup(Item, Lookup) ->
    Id = maps:get(<<"id">>, Item, <<>>),
    Slug = maps:get(<<"slug">>, Item, <<>>),
    Name = item_name(Item),
    lists:foldl(
      fun(Value, Acc) ->
          case normalize_exact(Value) of
              <<>> -> Acc;
              Key -> Acc#{Key => Item}
          end
      end,
      Lookup, [Id, Slug, Name]).

descriptor(Item) ->
    English = maps:get(<<"en">>, maps:get(<<"i18n">>, Item, #{}), #{}),
    Base = #{id => maps:get(<<"id">>, Item, null),
             slug => maps:get(<<"slug">>, Item, <<>>),
             name => item_name(Item), tradable => true},
    Descriptor = copy_fields(
      [{<<"gameRef">>, game_ref}, {<<"tags">>, tags}, {<<"ducats">>, ducats},
       {<<"bulkTradable">>, bulk_tradable}, {<<"maxRank">>, max_rank},
       {<<"maxCharges">>, max_charges}, {<<"maxAmberStars">>, max_amber_stars},
       {<<"maxCyanStars">>, max_cyan_stars}, {<<"subtypes">>, subtypes},
       {<<"setRoot">>, set_root}],
      Item,
      copy_fields([{<<"icon">>, icon}, {<<"thumb">>, thumb},
                   {<<"subIcon">>, sub_icon}], English, Base)),
    add_asset(Descriptor).

add_asset(Descriptor = #{id := Id}) when is_binary(Id) ->
    case maps:get(thumb, Descriptor, maps:get(icon, Descriptor, undefined)) of
        Image when is_binary(Image), byte_size(Image) > 0 ->
            Descriptor#{asset => #{id => <<"market-item:", Id/binary>>,
                                   source => <<"market">>,
                                   image_name => Image}};
        _ -> Descriptor
    end;
add_asset(Descriptor) -> Descriptor.

copy_fields(Fields, Source, Target) ->
    lists:foldl(
      fun({SourceKey, TargetKey}, Acc) ->
          case maps:find(SourceKey, Source) of
              {ok, undefined} -> Acc;
              {ok, null} -> Acc;
              {ok, Value} -> Acc#{TargetKey => Value};
              error -> Acc
          end
      end,
      Target, Fields).

item_name(Item) ->
    I18n = maps:get(<<"i18n">>, Item, #{}),
    English = maps:get(<<"en">>, I18n, #{}),
    maps:get(<<"name">>, English, maps:get(<<"slug">>, Item, <<>>)).

matches(Label, Candidates, Limit) ->
    NormalizedLabel = normalize(Label),
    Ranked = lists:sort(
      fun candidate_before/2,
      [score(NormalizedLabel, Candidate) || Candidate <- Candidates]),
    [public_match(Match) || Match <- lists:sublist(Ranked, Limit)].

score(Label, Candidate0 = #{name := Name, slug := Slug, normalized := Candidate}) ->
    Distance = levenshtein(Label, Candidate),
    Length = max(length(Label), length(Candidate)),
    Confidence = case Length of
        0 -> 1.0;
        _ -> round(max(0.0, 1.0 - Distance / Length) * 10000) / 10000
    end,
    Candidate0#{name => Name, slug => Slug, normalized => Candidate,
                match_class => match_class(Label, Candidate),
                distance => Distance, confidence => Confidence}.

candidate_before(Left, Right) ->
    LeftKey = {maps:get(match_class, Left), -maps:get(confidence, Left),
               maps:get(distance, Left),
               maps:get(normalized, Left), maps:get(slug, Left)},
    RightKey = {maps:get(match_class, Right), -maps:get(confidence, Right),
                maps:get(distance, Right),
                maps:get(normalized, Right), maps:get(slug, Right)},
    LeftKey < RightKey.

match_class(Value, Value) -> 0;
match_class(Label, Candidate) ->
    case lists:prefix(Label, Candidate) of
        true -> 1;
        false ->
            case token_prefix_match(string:tokens(Label, " "),
                                    string:tokens(Candidate, " ")) of
                true -> 2;
                false ->
                    case string:find(Candidate, Label) of
                        nomatch -> 4;
                        _ -> 3
                    end
            end
    end.

token_prefix_match([], _CandidateTokens) -> false;
token_prefix_match(LabelTokens, CandidateTokens) ->
    lists:all(
      fun(LabelToken) ->
          lists:any(fun(CandidateToken) -> lists:prefix(LabelToken, CandidateToken) end,
                    CandidateTokens)
      end,
      LabelTokens).

public_match(Candidate = #{name := Name, slug := Slug, distance := Distance,
                           confidence := Confidence}) ->
    Match = #{name => Name, slug => Slug, distance => Distance,
              confidence => Confidence},
    Match1 = lists:foldl(
      fun(Key, Acc) ->
          case maps:get(Key, Candidate, undefined) of
              undefined -> Acc;
              Value -> Acc#{Key => Value}
          end
      end,
      Match,
      [game_ref, icon, thumb, sub_icon]),
    case maps:get(ducats, Candidate, undefined) of
        Value when is_integer(Value), Value >= 0 -> Match1#{ducats => Value};
        _ -> Match1
    end.

normalize(Value) ->
    Lower = string:casefold(retinue_text:to_list(Value)),
    Normalized = re:replace(Lower, "[^\\p{L}\\p{N}]+", " ",
                            [global, unicode, ucp, {return, list}]),
    string:trim(Normalized).

normalize_exact(Value) ->
    unicode:characters_to_binary(
      string:casefold(string:trim(retinue_text:to_list(Value)))).

levenshtein(Left, Right) when length(Left) < length(Right) ->
    levenshtein(Right, Left);
levenshtein(Left, Right) ->
    Initial = lists:seq(0, length(Right)),
    {_RowNumber, Final} = lists:foldl(
      fun(Char, {RowNumber, Previous}) ->
          {RowNumber + 1, distance_row(Char, Right, Previous, RowNumber)}
      end,
      {1, Initial},
      Left),
    lists:last(Final).

distance_row(Char, Other, [PreviousHead | PreviousTail], RowNumber) ->
    lists:reverse(
      distance_cells(Char, Other, PreviousTail, PreviousHead, RowNumber,
                     [RowNumber])).

distance_cells(_Char, [], [], _Diagonal, _Left, Acc) -> Acc;
distance_cells(Char, [OtherChar | Other], [Up | Previous], Diagonal, Left, Acc) ->
    Substitution = Diagonal + case Char =:= OtherChar of true -> 0; false -> 1 end,
    Current = min(min(Up + 1, Left + 1), Substitution),
    distance_cells(Char, Other, Previous, Up, Current, [Current | Acc]).

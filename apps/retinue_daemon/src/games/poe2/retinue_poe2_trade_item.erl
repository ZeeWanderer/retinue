-module(retinue_poe2_trade_item).
-export([catalog/1, parse/2, from_listing/1, sane_number/1, merge_modifiers/1]).

-define(NUMBER, <<"([+-]?[0-9]+(?:\\.[0-9]+)?)">>).
-define(MAGNITUDE, <<?NUMBER/binary, "(?:\\(", ?NUMBER/binary, "(?:-", ?NUMBER/binary, ")?\\))?">>).

catalog(#{<<"result">> := Groups}) when is_list(Groups) ->
    try
        Entries = [compile_stat(Entry, maps:get(<<"id">>, Group))
                   || Group <- Groups, Entry <- maps:get(<<"entries">>, Group),
                      maps:get(<<"id">>, Group) =/= <<"pseudo">>],
        true = length(Entries) > 0 andalso length(Entries) =< 30000,
        {ok, lists:foldl(fun({Section, Stat}, Acc) ->
            Acc#{Section => [Stat | maps:get(Section, Acc, [])]}
        end, #{}, Entries)}
    catch _:_ -> {error, invalid_stats_metadata} end;
catalog(_) -> {error, invalid_stats_metadata}.

compile_stat(#{<<"id">> := Id, <<"text">> := Text} = Entry, Group)
  when is_binary(Id), is_binary(Text), byte_size(Text) =< 4096 ->
    true = byte_size(Id) > 0 andalso byte_size(Id) =< 160,
    [Group, _] = binary:split(Id, <<".">>),
    Section = case Group of <<"rune">> -> <<"augment">>; _ -> Group end,
    Plain = normalize(re:replace(Text, <<" \\(Local\\)$">>, <<>>, [{return, binary}])),
    Parts = binary:split(Plain, <<"#">>, [global]),
    Pattern = iolist_to_binary([<<"^">>, lists:join(?MAGNITUDE, [escape(P) || P <- Parts]), <<"$">>]),
    {ok, RE} = re:compile(Pattern, [unicode]),
    Count = length(Parts) - 1,
    Range = Count =:= 2 andalso binary:match(Plain, <<"# to #">>) =/= nomatch,
    {Section, #{id => Id, re => RE, count => Count, range => Range, template => Plain,
                lines => length(binary:split(Text, <<"\n">>, [global])),
                selectable => not maps:is_key(<<"option">>, Entry)}}.

escape(Text) ->
    re:replace(Text, <<"([\\\\.\\^\\$\\|\\(\\)\\[\\]\\{\\}\\*\\+\\?])">>, <<"\\\\\\1">>,
               [global, {return, binary}]).

parse(Text, Catalog) when is_binary(Text), byte_size(Text) > 0, byte_size(Text) =< 32768 ->
    try
        Text = unicode:characters_to_binary(Text),
        nomatch = re:run(Text, <<"[\\x00-\\x08\\x0b\\x0c\\x0e-\\x1f]">>),
        Plain = re:replace(Text, <<"\\[(?:[^|\\]]*\\|)?([^|\\]]+)\\]">>, <<"\\1">>,
                           [global, {return, binary}]),
        Lines = [string:trim(L) || L <- binary:split(Plain, <<"\n">>, [global])],
        true = length(Lines) =< 512,
        {Header, Rest} = lists:splitwith(fun(L) -> L =/= <<"--------">> end, Lines),
        {Rarity, Name, Base} = header(Header),
        true = byte_size(Base) > 0 andalso byte_size(Base) =< 256,
        true = byte_size(Name) =< 256,
        Sections = modifier_sections(Rest),
        {Props0, Mods0, Level} = body(Sections, Catalog, #{section => <<"explicit">>}, [], [], undefined, 0),
        {Properties, Modifiers} = case Rarity of
            <<"Gem">> -> gem_properties(lists:reverse(Props0));
            _ -> {lists:reverse(Props0), lists:reverse(Mods0)}
        end,
        Item = #{<<"name">> => Name, <<"base">> => Base, <<"rarity">> => Rarity,
                 <<"item_class">> => item_class(Header),
                 <<"properties">> => Properties,
                 <<"modifiers">> => [M#{<<"ranges">> => retinue_poe2_trade_profiles:ranges(M)}
                                     || M <- Modifiers],
                 <<"conditions">> => conditions(Rarity, Lines),
                 <<"corrupted">> => lists:member(<<"Corrupted">>, Lines),
                 <<"unidentified">> => lists:member(<<"Unidentified">>, Lines)},
        {ok, case Level of undefined -> Item; _ -> Item#{<<"item_level">> => Level} end}
    catch _:_ -> {error, invalid_item_text} end;
parse(_, _) -> {error, invalid_item_text}.

from_listing(Item) ->
    Frame = maps:get(<<"frameType">>, Item, -1),
    Rarity = maps:get(Frame, #{0 => <<"Normal">>, 1 => <<"Magic">>, 2 => <<"Rare">>,
                             3 => <<"Unique">>, 4 => <<"Gem">>, 5 => <<"Currency">>,
                             7 => <<"Quest">>}, <<"Unknown">>),
    Groups = [{<<"enchantMods">>, <<"enchant">>}, {<<"implicitMods">>, <<"implicit">>},
              {<<"runeMods">>, <<"augment">>}, {<<"fracturedMods">>, <<"fractured">>},
              {<<"explicitMods">>, <<"explicit">>}, {<<"craftedMods">>, <<"crafted">>},
              {<<"desecratedMods">>, <<"desecrated">>}],
    Mods = [#{<<"key">> => <<Section/binary, ":", (integer_to_binary(Index))/binary>>,
              <<"text">> => Text, <<"section">> => Section, <<"supported">> => false}
            || {Key, Section} <- Groups,
               {Index, Text} <- lists:enumerate(listing_fields(Key, Item)),
               is_binary(Text), byte_size(Text) =< 4096],
    Properties = [#{<<"text">> => listing_property(P)}
                  || P <- listing_fields(<<"properties">>, Item), is_map(P)],
    Requirements = [listing_property(P) || P <- listing_fields(<<"requirements">>, Item), is_map(P)],
    Flags = [Text || {Key, Text} <- [{<<"corrupted">>, <<"Corrupted">>},
                                    {<<"mirrored">>, <<"Mirrored">>},
                                    {<<"split">>, <<"Split">>},
                                    {<<"sanctified">>, <<"Sanctified">>}],
                     maps:get(Key, Item, false) =:= true],
    Identified = maps:get(<<"identified">>, Item, true) =/= false,
    State = case Identified of true -> Flags; false -> [<<"Unidentified">> | Flags] end,
    #{<<"name">> => listing_text(maps:get(<<"name">>, Item, <<>>)),
      <<"base">> => listing_text(maps:get(<<"typeLine">>, Item, maps:get(<<"baseType">>, Item, <<>>))),
      <<"rarity">> => Rarity, <<"item_level">> => case maps:get(<<"ilvl">>, Item, null) of
          Level when is_integer(Level), Level > 0, Level =< 100 -> Level;
          _ -> null
      end,
      <<"properties">> => Properties ++ case Requirements of
          [] -> [];
          _ -> [#{<<"text">> => iolist_to_binary([<<"Requires ">>, lists:join(<<", ">>, Requirements)])}]
      end,
      <<"modifiers">> => Mods, <<"conditions">> => conditions(Rarity, State),
      <<"corrupted">> => maps:get(<<"corrupted">>, Item, false) =:= true,
      <<"unidentified">> => not Identified}.

listing_fields(Key, Map) ->
    case maps:get(Key, Map, []) of List when is_list(List) -> lists:sublist(List, 128); _ -> [] end.
listing_text(Text) when is_binary(Text), byte_size(Text) =< 4096 -> Text;
listing_text(_) -> <<>>.

listing_property(Property) ->
    Name = listing_text(maps:get(<<"name">>, Property, <<>>)),
    Values = [listing_text(V) || [V, _] <- listing_fields(<<"values">>, Property)],
    case maps:get(<<"displayMode">>, Property, 0) of
        3 -> lists:foldl(fun({I, V}, Text) ->
                 binary:replace(Text, <<"{", (integer_to_binary(I))/binary, "}">>, V, [global])
             end, Name, lists:enumerate(0, Values));
        1 -> iolist_to_binary(lists:join(<<" ">>, Values ++ [Name]));
        2 -> iolist_to_binary(lists:join(<<", ">>, Values));
        _ when Values =:= [] -> Name;
        _ -> iolist_to_binary([Name, <<": ">>, lists:join(<<", ">>, Values)])
    end.

conditions(Rarity, Lines) ->
    States = [{<<"Unidentified">>, <<"identified">>, false},
              {<<"Mirrored">>, <<"mirrored">>, true},
              {<<"Split">>, <<"split">>, true},
              {<<"Unmodifiable">>, null, true},
              {<<"Sanctified">>, <<"sanctified">>, true},
              {<<"Corrupted">>, <<"corrupted">>, true},
              {<<"Twice Corrupted">>, <<"twice_corrupted">>, true}],
    Present = [#{<<"id">> => Key, <<"text">> => Text, <<"filters">> =>
                    case Key of null -> #{}; _ -> #{Key => Value} end}
               || {Text, Key, Value} <- States, lists:member(Text, Lines)],
    Blockers = [<<"Mirrored">>, <<"Unmodifiable">>, <<"Sanctified">>, <<"Corrupted">>, <<"Twice Corrupted">>],
    case lists:member(Rarity, [<<"Normal">>, <<"Magic">>, <<"Rare">>, <<"Unique">>])
         andalso not lists:any(fun(Text) -> lists:member(Text, Lines) end, Blockers) of
        true -> Present ++ [#{<<"id">> => <<"modifiable">>, <<"text">> => <<"Modifiable">>,
            <<"filters">> => #{<<"mirrored">> => false, <<"sanctified">> => false,
                               <<"corrupted">> => false, <<"twice_corrupted">> => false}}];
        false -> Present
    end.

header([<<"Item Class: ", _/binary>> | Rest]) -> header(Rest);
header([<<"Rarity: ", Rarity/binary>> | Names]) ->
    true = lists:member(Rarity, [<<"Normal">>, <<"Magic">>, <<"Rare">>, <<"Unique">>,
                               <<"Gem">>, <<"Currency">>, <<"Quest">>]),
    case [N || N <- Names, N =/= <<>>] of
        [Name, Base] when Rarity =:= <<"Rare">>; Rarity =:= <<"Unique">> -> {Rarity, Name, Base};
        [Base] -> {Rarity, <<>>, Base}
    end.

item_class([<<"Item Class: ", Class/binary>> | _]) -> Class;
item_class(_) -> null.

modifier_sections(Lines) ->
    Sections = split_sections(Lines),
    Decorated = lists:any(fun(Block) -> lists:any(fun
        (<<"{", _/binary>> = Line) ->
            lists:member(marker_section(Line), [<<"explicit">>, <<"fractured">>, <<"crafted">>, <<"desecrated">>]);
        (_) -> false
    end, Block) end, Sections),
    lists:append([[<<"--------">> | Block] || Block <- Sections,
        not Decorated orelse lists:any(fun
            (<<"{", _/binary>>) -> true;
            (Line) -> property(Line, 0) orelse element(1, section(Line, <<"explicit">>)) =/= <<"explicit">>
        end, Block) orelse lists:any(fun(Line) -> string:prefix(Line, <<"Item Level:">>) =/= nomatch end, Block)]).

split_sections([]) -> [];
split_sections([<<"--------">> | Rest]) -> split_sections(Rest);
split_sections(Lines) ->
    {Block, Rest} = lists:splitwith(fun(Line) -> Line =/= <<"--------">> end, Lines),
    [Block | split_sections(Rest)].

gem_properties(Properties) ->
    lists:foldr(fun(#{<<"text">> := Text} = Property, {Props, Filters}) ->
        case gem_filter(Text) of
            undefined -> {[Property | Props], Filters};
            {Id, Value} ->
                Row = #{<<"key">> => Id, <<"id">> => Id, <<"text">> => Text,
                        <<"section">> => <<"property">>, <<"min">> => Value, <<"max">> => Value,
                        <<"supported">> => true, <<"selected">> => true},
                {Props, [Row | Filters]}
        end
    end, {[], []}, Properties).

gem_filter(<<"Sockets: ", Sockets/binary>>) ->
    Tokens = string:lexemes(Sockets, " "),
    case Tokens =/= [] andalso lists:all(fun(T) -> lists:member(T, [<<"R">>, <<"G">>, <<"B">>, <<"W">>]) end, Tokens) of
        true -> {<<"property.gem_sockets">>, length(Tokens)};
        false -> undefined
    end;
gem_filter(Text) ->
    case re:run(Text, <<"^(Level|Quality): \\+?([0-9]+)(?:%|$| )">>, [{capture, [1, 2], binary}]) of
        {match, [Label, Number]} ->
            Id = case Label of <<"Level">> -> <<"property.gem_level">>; <<"Quality">> -> <<"property.quality">> end,
            {Id, binary_to_integer(Number)};
        nomatch -> undefined
    end.

body([], _, _, Props, Mods, Level, _) -> {Props, Mods, Level};
body([<<"--------">> | Rest], Catalog, _, Props, Mods, Level, Index) ->
    Block = lists:takewhile(fun(L) -> L =/= <<"--------">> end, Rest),
    Skip = Level =/= undefined andalso not modifier_block(Block, Catalog),
    body(Rest, Catalog, #{section => <<"explicit">>, skip => Skip}, Props, Mods, Level, Index);
body([_ | Rest], Catalog, #{skip := true} = Context, Props, Mods, Level, Index) ->
    body(Rest, Catalog, Context, Props, Mods, Level, Index);
body([<<>> | Rest], Catalog, Section, Props, Mods, Level, Index) ->
    body(Rest, Catalog, Section, Props, Mods, Level, Index);
body([<<"Item Level: ", Value/binary>> | Rest], Catalog, Section, Props, Mods, _, Index) ->
    Level = binary_to_integer(Value),
    true = Level > 0 andalso Level =< 100,
    body(Rest, Catalog, Section, Props, Mods, Level, Index);
body([Flag | Rest], Catalog, Section, Props, Mods, Level, Index)
  when Flag =:= <<"Corrupted">>; Flag =:= <<"Unidentified">> ->
    body(Rest, Catalog, Section, Props, Mods, Level, Index);
body([<<"{", _/binary>> = Line | Rest], Catalog, _, Props, Mods, Level, Index) ->
    Context = #{section => marker_section(Line), decorator => decorator(Line, Index)},
    body(Rest, Catalog, Context, Props, Mods, Level, Index);
body([Line | Rest], Catalog, Context = #{section := Section0}, Props, Mods, Level, Index) ->
    case property(Line, Level) of
        true -> body(Rest, Catalog, Context, [#{<<"text">> => Line} | Props], Mods, Level, Index);
        false ->
            {Section, Clean0} = section(Line, Section0),
            Clean = clean_modifier(Clean0),
            Stats = maps:get(Section, Catalog, []),
            {Text, Candidates, Remaining} = match_lines(Clean, Rest, Stats),
            Key = iolist_to_binary([Section, <<":">>, integer_to_binary(Index), <<":">>,
                binary:encode_hex(binary:part(crypto:hash(sha256, Text), 0, 8), lowercase)]),
            Row0 = #{<<"key">> => Key, <<"text">> => Text, <<"section">> => Section,
                    <<"magnitudes">> => [],
                    <<"id">> => null, <<"min">> => null, <<"max">> => null,
                    <<"supported">> => false},
            Row = case Context of
                #{decorator := Decorator} when Section =:= Section0 ->
                    Row0#{<<"decorator">> => Decorator};
                _ -> Row0
            end,
            Resolved = resolve(Row, Candidates),
            body(Remaining, Catalog, Context, Props, [Resolved | Mods], Level, Index + 1)
    end.

modifier_block([], _) -> false;
modifier_block([<<"{", _/binary>> | _], _) -> true;
modifier_block([Line | Rest], Catalog) ->
    {Section, Clean} = section(Line, <<"explicit">>),
    case match_lines(clean_modifier(Clean), Rest, maps:get(Section, Catalog, [])) of
        {_, [_ | _], _} -> true;
        _ -> modifier_block(Rest, Catalog)
    end.

clean_modifier(Text) ->
    Fixed = re:replace(Text, <<"\\s*\\x{2014} Unscalable Value$">>, <<>>,
                       [unicode, {return, binary}]),
    re:replace(Fixed, <<"([0-9])\\s+\\(([+-]?[0-9])">>, <<"\\1(\\2">>,
               [global, {return, binary}]).

resolve(Row, []) -> Row#{<<"resolution">> => <<"unknown">>};
resolve(Row, Candidates) ->
    Shapes = lists:usort([{Value, Supported, Rolls, Template} ||
        {_, Value, Supported, Rolls, Template} <- Candidates]),
    case Shapes of
        [{Value, Supported, Rolls, #{template := Template, values := Values, negated := Negated}}] ->
            Ids = lists:usort([Id || {Id, _, _, _, _} <- Candidates]),
            Id = iolist_to_binary(lists:join(<<",">>, Ids)),
            Text = re:replace(maps:get(<<"text">>, Row),
                <<"(?<=[0-9])\\([+-]?[0-9]+(?:\\.[0-9]+)?(?:-[+-]?[0-9]+(?:\\.[0-9]+)?)?\\)">>,
                <<>>, [global, {return, binary}]),
            Row#{<<"id">> => Id, <<"text">> => Text, <<"min">> => Value, <<"max">> => Value,
                 <<"supported">> => Supported, <<"magnitudes">> => Rolls,
                 <<"display_template">> => Template,
                 <<"display_values">> => Values, <<"negated">> => Negated,
                 <<"resolution">> => case Supported of true -> <<"resolved">>;
                                                       false -> <<"unsupported_filter">> end};
        _ -> Row#{<<"resolution">> => <<"ambiguous">>}
    end.

decorator(Line, Index) ->
    Raw = #{<<"raw">> => Line, <<"group">> => Index},
    Fields = [{<<"name">>, <<"\"([^\"]+)\"">>},
              {<<"affix">>, <<"(Prefix|Suffix) Modifier">>},
              {<<"tier">>, <<"\\((?:Tier|Rank): ([0-9]+)\\)">>},
              {<<"tags">>, <<"\\x{2014}\\s*([^}]+)\\s*\\}$">>}],
    lists:foldl(fun({Key, Pattern}, Acc) ->
        case re:run(Line, Pattern, [unicode, {capture, [1], binary}]) of
            {match, [Value]} ->
                case Key of
                    <<"tier">> ->
                        case string:to_integer(Value) of
                            {N, <<>>} when N > 0, N < 1000 -> Acc#{Key => N};
                            _ -> Acc
                        end;
                    <<"tags">> -> Acc#{Key => [Tag || Part <- binary:split(Value, <<",">>, [global]),
                        Tag <- [string:trim(Part)], Tag =/= <<>>]};
                    _ -> Acc#{Key => Value}
                end;
            nomatch -> Acc
        end
    end, Raw, Fields).

magnitude([Value, Min, Max], Sign) ->
    V = number(Value),
    {Lo, Hi} = case {Min, Max} of
        {<<>>, <<>>} -> {V, V};
        {_, <<>>} -> {number(Min), number(Min)};
        _ -> {number(Min), number(Max)}
    end,
    Low = min(Lo, Hi), High = max(Lo, Hi),
    case lists:all(fun sane_number/1, [V, Lo, Hi]) andalso Low =< V andalso V =< High of
        true when Sign =:= 1 -> #{<<"value">> => V, <<"min">> => Low, <<"max">> => High};
        true -> #{<<"value">> => -V, <<"min">> => -High, <<"max">> => -Low};
        false -> undefined
    end.

capture_triples([]) -> [];
capture_triples([Value, Min, Max | Rest]) -> [[Value, Min, Max] | capture_triples(Rest)];
capture_triples([Value]) -> [[Value, <<>>, <<>>]];
capture_triples([Value, Min]) -> [[Value, Min, <<>>]].

marker_section(Line) ->
    Header = hd(binary:split(Line, <<" Modifier">>)),
    Markers = [{<<"Fractured">>, <<"fractured">>}, {<<"Crafted">>, <<"crafted">>},
               {<<"Desecrated">>, <<"desecrated">>},
               {<<"Enchant">>, <<"enchant">>}, {<<"Corrupted Implicit">>, <<"enchant">>},
               {<<"Implicit Modifier">>, <<"implicit">>},
               {<<"Rune Modifier">>, <<"augment">>}, {<<"Augment Modifier">>, <<"augment">>},
               {<<"Unique Modifier">>, <<"explicit">>},
               {<<"Prefix Modifier">>, <<"explicit">>}, {<<"Suffix Modifier">>, <<"explicit">>}],
    case [Section || {Marker, Section} <- Markers,
                     binary:match(<<Header/binary, " Modifier">>, Marker) =/= nomatch] of
        [Section | _] -> Section;
        _ -> <<"unknown">>
    end.

property(Line, Level) ->
    Level =:= undefined orelse
    lists:member(Line, [<<"Corrupted">>, <<"Twice Corrupted">>, <<"Unidentified">>, <<"Mirrored">>,
        <<"Split">>, <<"Unmodifiable">>, <<"Sanctified">>, <<"Fractured Item">>]) orelse
    re:run(Line, <<"^(Quality|Armour|Evasion Rating|Energy Shield|Ward|Physical Damage|Elemental Damage|Chaos Damage|Critical Hit Chance|Critical Strike Chance|Attacks per Second|Reload Time|Sockets|Requires|Requirements|Level|Str|Dex|Int|Spirit|Block chance|Block Chance|Note):">>) =/= nomatch.

section(Line, Default) ->
    case re:run(Line, <<"^(.*) \\((implicit|explicit|rune|augment|crafted|fractured|enchant|desecrated)\\)$">>,
                [{capture, [1, 2], binary}]) of
        {match, [Text, <<"rune">>]} -> {<<"augment">>, Text};
        {match, [Text, Section]} -> {Section, Text};
        nomatch -> {Default, Line}
    end.

match_lines(Line, Rest, Stats) ->
    Max = lists:foldl(fun(#{lines := N}, Acc) -> max(N, Acc) end, 1, Stats),
    Extra = [clean_modifier(L) || L <- lists:sublist(lists:takewhile(fun(L) ->
        L =/= <<"--------">> andalso string:prefix(L, <<"{">>) =:= nomatch
    end, Rest), min(Max - 1, 7))],
    match_lengths(length(Extra) + 1, [Line | Extra], Rest, Stats).

match_lengths(0, [Line | _], Rest, _) -> {Line, [], Rest};
match_lengths(N, Lines, Rest, Stats) ->
    Text = iolist_to_binary(lists:join(<<"\n">>, lists:sublist(Lines, N))),
    Candidates = match(normalize(Text), [S || S = #{lines := L} <- Stats, L =:= N]),
    case Candidates of
        [] -> match_lengths(N - 1, Lines, Rest, Stats);
        _ -> {Text, Candidates, lists:nthtail(N - 1, Rest)}
    end.

match(Text, Stats) ->
    Direct = matches(Text, Stats, 1),
    case Direct of
        [] ->
            Increased = binary:replace(Text, <<" reduced ">>, <<" increased ">>, [global]),
            case Increased =/= Text of true -> matches(Increased, Stats, -1); false -> [] end;
        _ -> Direct
    end.

matches(Text, Stats, Sign) ->
    lists:filtermap(fun(#{re := RE, id := Id, count := Count, range := Range,
                         selectable := Selectable, template := Template}) ->
        case re:run(Text, RE, [{capture, all_but_first, binary}]) of
            nomatch -> false;
            {match, Captures} ->
                Tokens = capture_triples(Captures),
                Values = [number(V) * Sign || [V, _, _] <- Tokens],
                Ordered = case Values of [A0, B0] when Range -> A0 =< B0; _ -> true end,
                Supported = Selectable andalso lists:all(fun sane_number/1, Values) andalso
                            Ordered,
                Value = case Values of [] -> null; [V] -> V;
                    _ -> lists:sum(Values) / Count end,
                Rolls = [magnitude(Token, Sign) || Token <- Tokens],
                Magnitudes = case lists:member(undefined, Rolls) of true -> []; false -> Rolls end,
                {true, {Id, Value, Supported, Magnitudes,
                    #{template => Template, values => Values, negated => Sign =:= -1}}}
        end
    end, Stats).

normalize(Text) ->
    Spaced = re:replace(Text, <<"\\s+">>, <<" ">>, [global, {return, binary}]),
    string:trim(Spaced).

number(<<"+", Rest/binary>>) -> number(Rest);
number(Bin) ->
    case binary:match(Bin, <<".">>) of
        nomatch -> binary_to_integer(Bin);
        _ -> binary_to_float(Bin)
    end.

sane_number(N) -> is_number(N) andalso N >= -1000000000 andalso N =< 1000000000.

merge_modifiers(Rows) ->
    {Order, Groups} = lists:foldl(fun(Row, {Keys, Acc}) ->
        Key = case Row of
            #{<<"supported">> := true, <<"id">> := Id, <<"section">> := Section} -> {Section, Id};
            _ -> maps:get(<<"key">>, Row)
        end,
        case maps:find(Key, Acc) of
            error -> {[Key | Keys], Acc#{Key => Row}};
            {ok, First} -> {Keys, Acc#{Key => merge_modifier(First, Row)}}
        end
    end, {[], #{}}, Rows),
    [maps:get(Key, Groups) || Key <- lists:reverse(Order)].

merge_modifier(First, Row) ->
    Value = case {maps:get(<<"min">>, First, null), maps:get(<<"min">>, Row, null)} of
        {A, B} when is_number(A), is_number(B) -> A + B;
        _ -> null
    end,
    Merged = First#{<<"min">> => Value, <<"max">> => Value,
        <<"magnitudes">> => maps:get(<<"magnitudes">>, First, []) ++ maps:get(<<"magnitudes">>, Row, [])},
    Values = case {maps:get(<<"display_values">>, First, []), maps:get(<<"display_values">>, Row, [])} of
        {AValues, BValues} when length(AValues) =:= length(BValues) ->
            [A + B || {A, B} <- lists:zip(AValues, BValues)];
        _ -> []
    end,
    Text = case {binary:split(maps:get(<<"display_template">>, First, <<>>), <<"#">>, [global]), Values} of
        {[_ | _] = Parts, [_ | _]} when length(Parts) =:= length(Values) + 1 ->
            iolist_to_binary([hd(Parts) | [[format_number(N), P] || {N, P} <- lists:zip(Values, tl(Parts))]]);
        _ -> maps:get(<<"text">>, First)
    end,
    Flags = lists:foldl(fun(Key, Acc) ->
        case maps:is_key(Key, First) of
            true -> Acc#{Key => maps:get(Key, First) orelse maps:get(Key, Row, false)};
            false -> Acc
        end
    end, Merged, [<<"quick_selected">>, <<"exact_selected">>, <<"base_selected">>]),
    Flags#{<<"text">> => Text, <<"display_values">> => Values, <<"negated">> => false,
           <<"ranges">> => retinue_poe2_trade_profiles:ranges(Flags)}.

format_number(N) when is_integer(N) -> integer_to_binary(N);
format_number(N) when N == trunc(N) -> integer_to_binary(trunc(N));
format_number(N) -> float_to_binary(N, [short]).

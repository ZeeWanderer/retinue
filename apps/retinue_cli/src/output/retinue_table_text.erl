%%%-------------------------------------------------------------------
%% ANSI-aware table cell normalization, wrapping, and line formatting.
%%%-------------------------------------------------------------------
-module(retinue_table_text).

-export([sanitize/1, words/1, wrap_cell/3, format_line/2, format_rows/5]).

-doc "Collapse whitespace and normalize one value for table rendering.".
-spec sanitize(term()) -> string().
sanitize(Value) ->
    Str0 = retinue_text:to_list(Value),
    case unicode:characters_to_binary(Str0) of
        Bin when is_binary(Bin) ->
            Bin1 = re:replace(Bin, "\\s+", " ", [global, {return, binary}]),
            case unicode:characters_to_list(Bin1) of
                List when is_list(List) -> string:trim(List);
                _ -> string:trim(Str0)
            end;
        _ ->
            string:trim(Str0)
    end.

-doc "Wrap or trim one cell to display width.".
-spec wrap_cell(term(), integer(), boolean()) -> [string()].
wrap_cell(Value, Width, Wrap) ->
    Str = retinue_text:to_list(Value),
    case Wrap of
        false -> [trim_to_width(Str, Width)];
        true ->
            Lines = [trim_to_width(Line, Width) || Line <- wrap_text(Str, Width)],
            propagate_ansi_lines(Lines)
    end.

-doc "Split text into wrapping words, separating ISO date and time components.".
-spec words(string()) -> [string()].
words(Str) ->
    lists:flatmap(fun maybe_split_iso_word/1, string:tokens(Str, " ")).

-doc "Format one table row using fixed display widths.".
-spec format_line([term()], [non_neg_integer()]) -> string().
format_line(Row, Widths) ->
    format_line(Row, Widths, none, fun(Line, _Status) -> Line end).

-doc "Wrap and format all body rows, applying row status coloring.".
-spec format_rows([[term()]], [non_neg_integer()], [boolean()], [term()],
                  fun((string(), term()) -> string())) -> [string()].
format_rows(Rows, Widths, WrapMask, Statuses, ColorFun) ->
    lists:flatmap(
      fun({Row, Status}) ->
          Lines = wrap_row(Row, Widths, WrapMask),
          [format_line(Line, Widths, Status, ColorFun) || Line <- Lines]
      end,
      lists:zip(Rows, Statuses)).

wrap_row(Row, Widths, WrapMask) ->
    Cells = pad_row(Row, length(Widths)),
    Wrapped = lists:zipwith3(fun wrap_cell/3, Cells, Widths, WrapMask),
    Lines = lists:max([length(Cell) || Cell <- Wrapped] ++ [1]),
    [[nth_or_empty(Cell, N) || Cell <- Wrapped] || N <- lists:seq(1, Lines)].

pad_row(Row, Count) ->
    Row ++ lists:duplicate(max(0, Count - length(Row)), "").

nth_or_empty(List, N) ->
    case N =< length(List) of
        true -> lists:nth(N, List);
        false -> ""
    end.

wrap_text("", _Width) -> [""];
wrap_text(Str, Width) when Width =< 0 -> [Str];
wrap_text(Str, Width) ->
    wrap_words(words(Str), Width, "", []).

maybe_split_iso_word(Word) ->
    case iso_match(Word) of
        true ->
            case string:split(Word, "T", leading) of
                [Date, Time] -> [Date, Time];
                _ -> [Word]
            end;
        false ->
            [Word]
    end.

iso_match(Word) ->
    case unicode:characters_to_binary(Word) of
        Bin when is_binary(Bin) ->
            re:run(Bin, "^[0-9]{4}-[0-9]{2}-[0-9]{2}T",
                   [{capture, none}]) =:= match;
        _ ->
            false
    end.

wrap_words([], _Width, "", Acc) ->
    lists:reverse(Acc);
wrap_words([], _Width, Line, Acc) ->
    lists:reverse([Line | Acc]);
wrap_words([Word | Rest], Width, "", Acc) ->
    case retinue_tty:display_width(Word) =< Width of
        true ->
            wrap_words(Rest, Width, Word, Acc);
        false ->
            case lists:reverse(split_word(Word, Width)) of
                [] ->
                    wrap_words(Rest, Width, "", Acc);
                [Last | Head] ->
                    wrap_words(Rest, Width, Last, Head ++ Acc)
            end
    end;
wrap_words([Word | Rest], Width, Line, Acc) ->
    Needed = retinue_tty:display_width(Line) + 1 + retinue_tty:display_width(Word),
    case Needed =< Width of
        true -> wrap_words(Rest, Width, Line ++ " " ++ Word, Acc);
        false -> wrap_words([Word | Rest], Width, "", [Line | Acc])
    end.

split_word(Word, Width) when Width =< 0 ->
    [Word];
split_word(Word, Width) ->
    split_word(Word, Width, []).

split_word([], _Width, Acc) ->
    lists:reverse(Acc);
split_word(Word, Width, Acc) ->
    case retinue_tty:display_width(Word) =< Width of
        true ->
            lists:reverse([Word | Acc]);
        false ->
            case retinue_tty:has_ansi(Word) of
                true ->
                    lists:reverse([Word | Acc]);
                false ->
                    case split_by_separator(Word, Width, $/) of
                        {Head, Tail} ->
                            split_word(Tail, Width, [Head | Acc]);
                        none ->
                            {Head, Tail} = split_by_width(Word, Width),
                            split_word(Tail, Width, [Head | Acc])
                    end
            end
    end.

split_by_separator(_Word, Width, _Separator) when Width =< 0 ->
    none;
split_by_separator(Word, Width, Separator) ->
    case find_last_separator(Word, Width, Separator, 0, 0) of
        0 -> none;
        Position ->
            {lists:sublist(Word, Position), lists:nthtail(Position, Word)}
    end.

find_last_separator([], _Width, _Separator, _Index, Last) ->
    Last;
find_last_separator(_Word, Width, _Separator, Index, Last) when Index >= Width ->
    Last;
find_last_separator([Char | Rest], Width, Separator, Index, Last) ->
    NewLast =
        case Char =:= Separator of
            true -> Index + 1;
            false -> Last
        end,
    find_last_separator(Rest, Width, Separator, Index + 1, NewLast).

trim_to_width(Str, Width) ->
    trim_to_width(Str, Width, []).

trim_to_width(_Str, Width, Acc) when Width =< 0 ->
    lists:reverse(Acc);
trim_to_width([], _Width, Acc) ->
    lists:reverse(Acc);
trim_to_width([$\e, $[ | Rest], Width, Acc) ->
    {Sequence, Tail} = ansi_sequence(Rest),
    trim_to_width(Tail, Width, lists:reverse(Sequence, Acc));
trim_to_width([Char | Rest], Width, Acc) ->
    CharWidth = retinue_tty:display_width([Char]),
    case Width - CharWidth >= 0 of
        true -> trim_to_width(Rest, Width - CharWidth, [Char | Acc]);
        false -> lists:reverse(Acc)
    end.

propagate_ansi_lines(Lines) ->
    propagate_ansi_lines(Lines, "").

propagate_ansi_lines([], _State) ->
    [];
propagate_ansi_lines([Line | Rest], State) ->
    Styled =
        case State of
            "" -> Line;
            _ -> State ++ Line
        end,
    {NewState, Output} = line_ansi_state(Styled),
    [Output | propagate_ansi_lines(Rest, NewState)].

line_ansi_state(Line) ->
    case last_sgr(Line) of
        "" -> {"", Line};
        "\e[0m" -> {"", Line};
        Sgr -> {Sgr, ensure_reset(Line)}
    end.

ensure_reset(Line) ->
    case string:find(Line, "\e[0m") of
        nomatch -> Line ++ "\e[0m";
        _ -> Line
    end.

last_sgr(Line) ->
    case last_sgr(Line, "") of
        {ok, Sgr} -> Sgr;
        error -> ""
    end.

last_sgr([], "") ->
    error;
last_sgr([], Last) ->
    {ok, Last};
last_sgr([$\e, $[ | Rest], _Last) ->
    {Sequence, Tail} = ansi_sequence(Rest),
    last_sgr(Tail, Sequence);
last_sgr([_ | Rest], Last) ->
    last_sgr(Rest, Last).

ansi_sequence(Rest) ->
    ansi_sequence(Rest, [$[, $\e]).

ansi_sequence([], Acc) ->
    {lists:reverse(Acc), []};
ansi_sequence([Char | Tail], Acc) ->
    Next = [Char | Acc],
    case Char of
        $m -> {lists:reverse(Next), Tail};
        _ -> ansi_sequence(Tail, Next)
    end.

split_by_width(Word, Width) ->
    split_by_width(Word, Width, 0, [], []).

split_by_width([], _Width, _CurrentWidth, Head, Tail) ->
    {lists:reverse(Head), lists:reverse(Tail)};
split_by_width([Char | Rest], Width, CurrentWidth, Head, Tail) ->
    CharWidth = retinue_tty:display_width([Char]),
    case CurrentWidth + CharWidth =< Width orelse Head =:= [] of
        true ->
            split_by_width(Rest, Width, CurrentWidth + CharWidth,
                           [Char | Head], Tail);
        false ->
            split_by_width(Rest, Width, CurrentWidth, Head, [Char | Tail])
    end.

format_line(Row, Widths, Status, ColorFun) ->
    Cells = lists:zipwith(fun pad_right/2, Row, Widths),
    ColorFun(string:join(Cells, "  "), Status).

pad_right(Value, Width) ->
    Str = retinue_text:to_list(Value),
    Padding = max(Width - retinue_tty:display_width(Str), 0),
    Str ++ lists:duplicate(Padding, $ ).

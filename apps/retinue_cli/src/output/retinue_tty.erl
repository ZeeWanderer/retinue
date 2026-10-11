%%%-------------------------------------------------------------------
%% Terminal helpers for widths and ANSI colorization.
%%%-------------------------------------------------------------------
-module(retinue_tty).

-export([terminal_width/0, display_width/1, strip_ansi/1, has_ansi/1,
         colorize/2, colorize_dt_tags/1, clear_screen/0, pad_right/2]).

terminal_width() ->
    case io:columns() of
        {ok, Columns} when is_integer(Columns), Columns > 0 -> Columns;
        _ -> 120
    end.

display_width(V) ->
    Str0 = strip_ansi(to_list(V)),
    Str = try unicode:characters_to_list(Str0) of
        L when is_list(L) -> L;
        _ -> Str0
    catch
        _:_ -> Str0
    end,
    unicode_width(Str).

unicode_width(Str) when is_list(Str) ->
    lists:sum([char_width(C) || C <- Str]);
unicode_width(_) ->
    0.

char_width(C) when is_integer(C, 0, 31) -> 0;
char_width(127) -> 0;
char_width(C) when is_integer(C) ->
    case is_combining(C) of
        true -> 0;
        false ->
            case is_wide(C) of
                true -> 2;
                false -> 1
            end
    end;
char_width(_) ->
    1.

is_combining(C) ->
    is_integer(C, 16#0300, 16#036F) orelse
    is_integer(C, 16#1AB0, 16#1AFF) orelse
    is_integer(C, 16#1DC0, 16#1DFF) orelse
    is_integer(C, 16#20D0, 16#20FF) orelse
    is_integer(C, 16#FE20, 16#FE2F).

is_wide(C) ->
    is_integer(C, 16#1100, 16#115F) orelse
    is_integer(C, 16#2329, 16#232A) orelse
    is_integer(C, 16#2E80, 16#A4CF) orelse
    is_integer(C, 16#AC00, 16#D7A3) orelse
    is_integer(C, 16#F900, 16#FAFF) orelse
    is_integer(C, 16#FE10, 16#FE19) orelse
    is_integer(C, 16#FE30, 16#FE6F) orelse
    is_integer(C, 16#FF00, 16#FF60) orelse
    is_integer(C, 16#FFE0, 16#FFE6).

pad_right(Str0, Width) ->
    Str = to_list(Str0),
    Padding = max(Width - display_width(Str), 0),
    Str ++ lists:duplicate(Padding, $ ).

strip_ansi(Str0) ->
    Str = to_list(Str0),
    unicode:characters_to_list(
      io_ansi:render(io_ansi:scan(Str), [{enabled, false}])).

has_ansi(Str0) ->
    lists:any(
      fun(Item) -> is_atom(Item) orelse is_tuple(Item) end,
      io_ansi:scan(to_list(Str0))).

-doc "Color terminal text using an OTP virtual-terminal sequence name.".
-spec colorize(unicode:chardata(), atom() | tuple()) -> string().
colorize(Text, Color) ->
    binary_to_list(
      io_ansi:format([Color, "~ts"], [Text],
                     [{enabled, true}, {color, true}])).

-doc "Clear standard output's terminal and return its cursor home.".
-spec clear_screen() -> ok.
clear_screen() ->
    io_ansi:fwrite([clear], [], [{enabled, true}]).

colorize_dt_tags(Str0) ->
    Str = lists:flatten(replace_special_tags(to_list(Str0))),
    lists:flatten(colorize_dt_tags(Str, [])).

colorize_dt_tags([], Acc) ->
    lists:reverse(Acc);
colorize_dt_tags(Str, Acc) ->
    case re:run(Str, "<DT_[A-Z_]+(?:_COLOR)?>", [{capture, first, index}, unicode]) of
        nomatch ->
            lists:reverse([Str | Acc]);
        {match, [{Start, Len}]} ->
            Prefix = lists:sublist(Str, Start),
            Rest0 = lists:nthtail(Start, Str),
            TagPart = lists:sublist(Rest0, Len),
            Rest1 = lists:nthtail(Len, Rest0),
            Rest = drop_gt(Rest1),
            Tag = tag_name(TagPart),
            {Word, Tail} = split_first_word(Rest),
            case Word of
                "" ->
                    colorize_dt_tags(Tail, [Prefix | Acc]);
                _ ->
                    Color = dt_color_code(Tag),
                    Colored = case Color of
                        "" -> Word;
                        % Reset styling only; io_ansi:reset/0 also resets the character set.
                        _ -> Color ++ Word ++ "\e[0m"
                    end,
                    colorize_dt_tags(Tail, [Colored, Prefix | Acc])
            end
    end.

tag_name(TagPart) ->
    Tag0 = string:trim(TagPart, both, "<>"),
    Tag1 = string:replace(Tag0, "DT_", "", all),
    Tag2 = string:replace(Tag1, "_COLOR", "", all),
    string:replace(Tag2, "COLOR", "", all).

split_first_word(Rest) ->
    split_first_word(Rest, []).

split_first_word([], Acc) ->
    {lists:reverse(Acc), ""};
split_first_word([$\s | Rest], Acc) ->
    {lists:reverse(Acc), [$\s | Rest]};
split_first_word([$; | Rest], Acc) ->
    {lists:reverse(Acc), [$; | Rest]};
split_first_word([C | Rest], Acc) ->
    split_first_word(Rest, [C | Acc]).

drop_gt([$> | Rest]) -> Rest;
drop_gt(Rest) -> Rest.

dt_color_code(Tag0) ->
    Tag = string:uppercase(to_list(Tag0)),
    case Tag of
        "POISON" -> rgb_ansi(70, 104, 47);
        "TOXIN" -> rgb_ansi(70, 104, 47);
        "GAS" -> rgb_ansi(50, 119, 98);
        "CORROSIVE" -> rgb_ansi(77, 102, 0);
        "COLD" -> rgb_ansi(23, 101, 140);
        "FREEZE" -> rgb_ansi(23, 101, 140);
        "ICE" -> rgb_ansi(23, 101, 140);
        "FIRE" -> rgb_ansi(153, 77, 0);
        "HEAT" -> rgb_ansi(153, 77, 0);
        "ELECTRIC" -> rgb_ansi(97, 15, 179);
        "ELECTRICITY" -> rgb_ansi(97, 15, 179);
        "BLAST" -> rgb_ansi(179, 42, 0);
        "EXPLOSION" -> rgb_ansi(179, 42, 0);
        "MAGNETIC" -> rgb_ansi(71, 71, 209);
        "VIRAL" -> rgb_ansi(183, 22, 88);
        "RADIANT" -> rgb_ansi(128, 96, 0);
        "RADIATION" -> rgb_ansi(128, 96, 0);
        "VOID" -> rgb_ansi(8, 94, 80);
        "IMPACT" -> rgb_ansi(61, 94, 94);
        "PUNCTURE" -> rgb_ansi(92, 82, 71);
        "SLASH" -> rgb_ansi(122, 82, 84);
        "SENTIENT" -> rgb_ansi(133, 97, 70);
        _ -> ""
    end.

replace_special_tags(Str) ->
    Replacements = [
        {"<LINE_SEPARATOR>", "\n"},
        {"<LOWER_IS_BETTER>", "(lower is better) "},
        {"<ACTIVATE_ABILITY_1>", "Activate Ability 1"},
        {"<SECONDARY_FIRE>", "Secondary Fire"},
        {"<AFFINITY_SHARE>", "Affinity Share"},
        {"<ENERGY>", "Energy"},
        {"<SHIELD>", "Shield"},
        {"<USE>", "Use"},
        {"<MADURAI_CLEAN>", "Madurai"},
        {"<NARAMON_CLEAN>", "Naramon"},
        {"<UNAIRU_CLEAN>", "Unairu"},
        {"<VAZARIN_CLEAN>", "Vazarin"},
        {"<ZENURIK_CLEAN>", "Zenurik"}
    ],
    lists:foldl(
      fun({Tag, Value}, Acc) -> string:replace(Acc, Tag, Value, all) end,
      Str,
      Replacements).

rgb_ansi(R, G, B) ->
    {RB, GB, BB} = brighten_rgb(R, G, B, 1.35),
    binary_to_list(io_ansi:color(RB, GB, BB)).

brighten_rgb(R, G, B, Factor) ->
    {clamp_rgb(R, Factor), clamp_rgb(G, Factor), clamp_rgb(B, Factor)}.

clamp_rgb(Value, Factor) ->
    Bright = trunc(Value * Factor),
    min(255, max(0, Bright)).

to_list(V) -> retinue_text:to_list(V).

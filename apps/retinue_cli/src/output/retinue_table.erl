%%%-------------------------------------------------------------------
%% Shared table rendering helpers.
%%%-------------------------------------------------------------------
-module(retinue_table).

-export([render_lines/3]).

render_lines(Headers0, Rows0, Opts) ->
    {Headers1, Rows1, Opts1} = prepare_table(Headers0, Rows0, Opts),
    Opts2 = Opts1#{prepared => true},
    Opts3 = derive_layout_opts(Headers1, Rows1, Opts2),
    case maybe_group_tables(Headers1, Rows1, Opts3) of
        {grouped, Lines} -> Lines;
        none -> render_lines_ungrouped(Headers1, Rows1, Opts3)
    end.

render_lines_ungrouped(Headers0, Rows0, Opts) ->
    {Headers1, Rows1, Opts1} = case maps:get(prepared, Opts, false) of
        true -> {Headers0, Rows0, Opts};
        false ->
            {HeadersPrep, RowsPrep, OptsPrep} = prepare_table(Headers0, Rows0, Opts),
            {HeadersPrep, RowsPrep, derive_layout_opts(HeadersPrep, RowsPrep, OptsPrep)}
    end,
    Headers = pad_row(Headers1, max_cols(Headers1, Rows1)),
    Rows = [pad_row(R, length(Headers)) || R <- Rows1],
    TermWidth = maps:get(width, Opts1, retinue_tty:terminal_width()),
    Statuses = pad_statuses(maps:get(row_statuses, Opts1, []), length(Rows)),
    ColorFun = maps:get(status_color_fun, Opts1, fun(Line, _Status) -> Line end),
    Layout0 = #{
        headers => Headers,
        rows => Rows,
        wrap_mask => pad_row(maps:get(wrap_mask, Opts1, []), length(Headers)),
        priorities => pad_row(maps:get(column_priorities, Opts1, default_priorities(length(Headers))), length(Headers)),
        min_widths => pad_row(expand_min_widths(maps:get(wrap_min_widths, Opts1, #{}), length(Headers)), length(Headers)),
        max_widths => pad_row(expand_max_widths(maps:get(wrap_max_widths, Opts1, #{}), length(Headers)), length(Headers)),
        required => maps:get(required_columns, Opts1, [])
    },
    Context = #{term_width => TermWidth, drop_columns => maps:get(drop_columns, Opts1, true), mode => wrap},
    Layout1 = apply_layout_rules(Layout0, Opts1, Context),
    Headers2 = maps:get(headers, Layout1),
    Rows2 = maps:get(rows, Layout1),
    WrapMask2 = maps:get(wrap_mask, Layout1),
    Priorities2 = maps:get(priorities, Layout1),
    MinWidths2 = maps:get(min_widths, Layout1),
    MaxWidths2 = maps:get(max_widths, Layout1),
    FlexColumns1 = normalize_flex_columns(maps:get(flex_columns, Opts1, []), Headers2, WrapMask2, Priorities2),
    Widths1 = wrap_widths(Headers2, Rows2, WrapMask2, TermWidth, MinWidths2,
                          MaxWidths2, true, Priorities2, FlexColumns1,
                          true, maps:get(reflow_max_iter, Opts1, 6)),
    HeaderLine = retinue_table_text:format_line(Headers2, Widths1),
    SepLine = retinue_table_text:format_line(
        [lists:duplicate(W, $-) || W <- Widths1], Widths1),
    BodyLines = retinue_table_text:format_rows(
        Rows2, Widths1, WrapMask2, Statuses, ColorFun),
    [HeaderLine, SepLine | BodyLines].

normalize_table(Headers0, Rows0) ->
    Headers = pad_row(
        [retinue_table_text:sanitize(H) || H <- Headers0], max_cols(Headers0, Rows0)),
    Rows = [pad_row([retinue_table_text:sanitize(V) || V <- R], length(Headers))
            || R <- Rows0],
    DropIdxs = duplicate_raw_indexes(Rows, Headers, length(Headers)),
    Headers1 = drop_indexes(Headers, DropIdxs),
    Rows1 = [drop_indexes(Row, DropIdxs) || Row <- Rows],
    {Headers1, Rows1, DropIdxs}.

apply_preferred_order(Headers, Rows, []) ->
    {Headers, Rows, lists:seq(1, length(Headers))};
apply_preferred_order(Headers, Rows, Preferred) ->
    Order = preferred_order_indexes(Headers, Preferred),
    {reorder_by(Headers, Order), [reorder_by(Row, Order) || Row <- Rows], Order}.

preferred_order_indexes(Headers, Preferred) ->
    HeaderMap = lists:foldl(
      fun({Idx, Label}, Acc) ->
          Acc#{string:lowercase(retinue_text:to_list(Label)) => Idx}
      end,
      #{},
      lists:zip(lists:seq(1, length(Headers)), Headers)),
    PreferredIdxs = lists:flatmap(
      fun(Col) ->
          case Col of
              Idx when is_integer(Idx, 1, length(Headers)) -> [Idx];
              _ ->
                  Key = string:lowercase(retinue_text:to_list(Col)),
                  case maps:get(Key, HeaderMap, undefined) of
                      undefined -> [];
                      Idx -> [Idx]
                  end
          end
      end,
      Preferred),
    Remaining = [Idx || Idx <- lists:seq(1, length(Headers)), not lists:member(Idx, PreferredIdxs)],
    PreferredIdxs ++ Remaining.

prepare_table(Headers0, Rows0, Opts0) ->
    {Headers1, Rows1, DropIdxs} = normalize_table(Headers0, Rows0),
    Opts1 = drop_specs_for_indexes(Opts0, DropIdxs),
    PreferredOrder = maps:get(preferred_order, Opts1, []),
    {Headers2, Rows2, Order} = apply_preferred_order(Headers1, Rows1, PreferredOrder),
    Opts2 = reorder_specs_for_order(Opts1, Order),
    {Headers2, Rows2, Opts2}.

drop_specs_for_indexes(Opts, []) -> Opts;
drop_specs_for_indexes(Opts, DropIdxs) ->
    case maps:get(column_specs, Opts, undefined) of
        Specs when is_list(Specs) ->
            Opts#{column_specs := drop_indexes(Specs, DropIdxs)};
        _ ->
            Opts
    end.

reorder_specs_for_order(Opts, Order) ->
    case maps:get(column_specs, Opts, undefined) of
        Specs when is_list(Specs), length(Specs) =:= length(Order) ->
            Opts#{column_specs := reorder_by(Specs, Order)};
        _ ->
            Opts
    end.

derive_layout_opts(Headers, Rows, Opts) ->
    TermWidth = maps:get(width, Opts, retinue_tty:terminal_width()),
    Specs = normalize_column_specs(Headers, Opts),
    RowMaps = maps:get(row_maps, Opts, undefined),
    Stats = column_stats_with_parts(Headers, Rows, RowMaps, Specs),
    HasSpecs = lists:any(fun(Spec) -> map_size(Spec) > 0 end, Specs),
    WrapMask = [
        should_wrap(S, TermWidth, Spec)
        || {S, Spec} <- lists:zip(Stats, Specs)
    ],
    MinWidths0 = [min_width(S, Wrap, Spec) || {S, Wrap, Spec} <- lists:zip3(Stats, WrapMask, Specs)],
    {WrapMask1, MinWidths1} = maybe_wrap_time_columns(Stats, Specs, WrapMask, MinWidths0, TermWidth),
    MinWidths = adjust_min_widths_for_details(Stats, Specs, WrapMask1, MinWidths1, TermWidth),
    MaxWidths = [max_width(S) || S <- Stats],
    Priorities = case HasSpecs of
        true ->
            [
                column_priority_score(S, Spec, Idx, length(Stats))
                || {Idx, {S, Spec}} <- lists:zip(lists:seq(1, length(Stats)), lists:zip(Stats, Specs))
            ];
        false ->
            [
                column_priority_score(S, Idx, length(Stats))
                || {Idx, S} <- lists:zip(lists:seq(1, length(Stats)), Stats)
            ]
    end,
    Required = [],
    FlexColumns = case HasSpecs of
        true -> flex_columns_from_specs(Headers, Specs, WrapMask1, Stats);
        false -> flex_columns_from_stats(Headers, Stats, WrapMask1)
    end,
    SplitLabel = case should_auto_split(Headers, Rows, Stats, case HasSpecs of true -> Specs; false -> [] end) of
        true -> "Type";
        false -> undefined
    end,
    DropUniformLabels = drop_uniform_labels_from_specs(case HasSpecs of true -> Specs; false -> [] end, Opts),
    Base = maps:with([width, row_statuses, status_color_fun, column_specs, row_maps,
                      group_resolver, group_label_format], Opts),
    Base#{
        wrap_mask => WrapMask1,
        wrap_min_widths => list_to_index_map(MinWidths),
        wrap_max_widths => list_to_index_map(MaxWidths),
        reflow_max_iter => 6,
        column_priorities => Priorities,
        required_columns => Required,
        flex_columns => FlexColumns,
        auto_split_by_label => SplitLabel,
        drop_uniform_labels => DropUniformLabels,
        tail_columns => tail_columns_from_specs(case HasSpecs of true -> Specs; false -> [] end, Opts)
    }.

max_cols(Headers, Rows) ->
    lists:max([length(Headers) | [length(R) || R <- Rows] ++ [0]]).

pad_row(Row, Count) ->
    case length(Row) >= Count of
        true -> lists:sublist(Row, Count);
        false -> Row ++ lists:duplicate(Count - length(Row), "")
    end.

pad_statuses(Statuses, Count) ->
    case length(Statuses) >= Count of
        true -> lists:sublist(Statuses, Count);
        false -> Statuses ++ lists:duplicate(Count - length(Statuses), none)
    end.

duplicate_raw_indexes(Rows, _Headers, ColCount) ->
    Columns = normalized_columns_raw(Rows, ColCount),
    {_Keep, Drop} = lists:foldl(
      fun(Idx, {KeepIdxs, DropIdxs}) ->
          case duplicate_of_kept(Idx, KeepIdxs, Columns) of
              none -> {[Idx | KeepIdxs], DropIdxs};
              _DupIdx -> {KeepIdxs, [Idx | DropIdxs]}
          end
      end,
      {[], []},
      lists:seq(1, ColCount)),
    lists:reverse(Drop).

duplicate_of_kept(_Idx, [], _Columns) -> none;
duplicate_of_kept(Idx, KeepIdxs, Columns) ->
    case lists:dropwhile(
           fun(I) -> not column_duplicate_raw(Columns, I, Idx) end,
           KeepIdxs) of
        [] -> none;
        [First | _] -> First
    end.

column_duplicate_raw(Columns, LeftIdx, RightIdx) ->
    Left = lists:nth(LeftIdx, Columns),
    Right = lists:nth(RightIdx, Columns),
    {AllMatch, HasValue} = lists:foldl(
      fun({L, R}, {Ok, Seen}) ->
          case R of
              "" -> {Ok, Seen};
              _ when R =:= L -> {Ok, true};
              _ -> {false, Seen}
          end
      end,
      {true, false},
      lists:zip(Left, Right)),
    AllMatch andalso HasValue.

normalized_columns_raw(Rows, ColCount) ->
    lists:map(
      fun(Idx) ->
          [normalize_raw_cell(lists:nth(Idx, Row)) || Row <- Rows]
      end,
      lists:seq(1, ColCount)).

normalize_raw_cell(Value) ->
    string:lowercase(string:trim(retinue_text:to_list(Value))).

column_stats(Headers, Rows) ->
    ColumnCount = length(Headers),
    [column_stat(Idx, Headers, Rows) || Idx <- lists:seq(1, ColumnCount)].

column_stats_with_parts(Headers, Rows, undefined, _Specs) ->
    column_stats(Headers, Rows);
column_stats_with_parts(Headers, Rows, RowMaps, Specs) ->
    ColumnCount = length(Headers),
    lists:map(
      fun(Idx) ->
          Header = lists:nth(Idx, Headers),
          Spec = case Specs of [] -> #{}; _ -> lists:nth(Idx, Specs) end,
          Parts = parts_from_spec(Spec),
          case Parts of
              [_ | _] ->
                  Values = part_values(RowMaps, Parts),
                  case all_empty_values(Values) of
                      true -> column_stat(Idx, Headers, Rows);
                      false -> column_stat_values(Header, Values)
                  end;
              _ ->
                  column_stat(Idx, Headers, Rows)
          end
      end,
      lists:seq(1, ColumnCount)).

column_stat(Idx, Headers, Rows) ->
    Header = lists:nth(Idx, Headers),
    HeaderW = retinue_tty:display_width(Header),
    Cells = [lists:nth(Idx, Row) || Row <- Rows],
    Init = #{fill_count => 0, unique => #{}, max_width => HeaderW, max_word => 0,
             has_space => false, has_slash => false},
    Stats = lists:foldl(fun fold_cell_stat/2, Init, Cells),
    UniqueCount = map_size(maps:get(unique, Stats)),
    Stats#{
        header => Header,
        header_width => HeaderW,
        unique_count => UniqueCount
    }.

column_stat_values(Header, Values) ->
    HeaderW = retinue_tty:display_width(Header),
    Init = #{fill_count => 0, unique => #{}, max_width => HeaderW, max_word => 0,
             has_space => false, has_slash => false},
    Stats = lists:foldl(fun fold_cell_stat/2, Init, Values),
    UniqueCount = map_size(maps:get(unique, Stats)),
    Stats#{
        header => Header,
        header_width => HeaderW,
        unique_count => UniqueCount
    }.

part_values(RowMaps, Parts) ->
    lists:map(
      fun(RowMap) ->
          Values = [retinue_text:to_list(maps:get(Key, RowMap, "")) || Key <- Parts],
          Present = [V || V <- Values, string:trim(V) =/= ""],
          retinue_text:join_list(Present, " .. ")
      end,
      RowMaps).

all_empty_values(Values) ->
    lists:all(fun(V) -> string:trim(retinue_text:to_list(V)) =:= "" end, Values).
fold_cell_stat(Cell, Acc0) ->
    Str = retinue_text:to_list(Cell),
    Present = cell_present(Str),
    Width = retinue_tty:display_width(Str),
    MaxWidth = max(maps:get(max_width, Acc0), Width),
    MaxWord = max(maps:get(max_word, Acc0), max_word_width(Str)),
    HasSpace = maps:get(has_space, Acc0) orelse string:find(Str, " ") =/= nomatch,
    HasSlash = maps:get(has_slash, Acc0) orelse string:find(Str, "/") =/= nomatch,
    Unique0 = maps:get(unique, Acc0),
    Unique1 = case Present of
        true -> Unique0#{Str => true};
        false -> Unique0
    end,
    FillCount = maps:get(fill_count, Acc0) + (if Present -> 1; true -> 0 end),
    Acc0#{
        fill_count => FillCount,
        unique => Unique1,
        max_width => MaxWidth,
        max_word => MaxWord,
        has_space => HasSpace,
        has_slash => HasSlash
    }.

max_word_width(Str) ->
    Words = retinue_table_text:words(Str),
    lists:max([retinue_tty:display_width(W) || W <- Words] ++ [0]).

cell_present(Value) ->
    Str = string:trim(retinue_text:to_list(Value)),
    Str =/= "" andalso Str =/= "null".

should_wrap(Stats, TermWidth, Spec) ->
    MaxWidth = maps:get(max_width, Stats),
    HasSpace = maps:get(has_space, Stats),
    HasSlash = maps:get(has_slash, Stats),
    HeaderW = maps:get(header_width, Stats),
    Header = maps:get(header, Stats),
    Identity = is_identity_label(Header),
    case wrap_for_spec(Spec) of
        true -> true;
        false ->
            case maps:get(kind, Spec, undefined) of
                time_range -> MaxWidth > TermWidth;
                time_point -> MaxWidth > TermWidth;
                _ ->
                    case Identity andalso MaxWidth =< max(TermWidth div 3, HeaderW) of
                        true -> false;
                        false -> HasSpace orelse HasSlash orelse MaxWidth > max(TermWidth div 2, HeaderW)
                    end
            end
    end.

is_identity_label(Label) ->
    Key = string:lowercase(retinue_text:to_list(Label)),
    lists:member(Key, ["name", "summary"]).

min_width(Stats, true, Spec) ->
    Base = maps:get(header_width, Stats),
    case maps:get(role, Spec, undefined) of
        location -> max(Base, maps:get(max_word, Stats));
        _ -> Base
    end;
min_width(Stats, false, _Spec) ->
    max(maps:get(header_width, Stats), maps:get(max_width, Stats)).

max_width(Stats) ->
    max(maps:get(header_width, Stats), maps:get(max_width, Stats)).

maybe_wrap_time_columns(Stats, Specs, WrapMask, MinWidths, TermWidth) ->
    Cols = length(MinWidths),
    Sep = case Cols of
        0 -> 0;
        _ -> (Cols - 1) * 2
    end,
    TotalMin = lists:sum(MinWidths) + Sep,
    case TotalMin =< TermWidth of
        true -> {WrapMask, MinWidths};
        false ->
            WrapMask1 = lists:zipwith(
              fun(Wrap, Spec) ->
                  case Wrap of
                      true -> true;
                      false ->
                          case maps:get(kind, Spec, undefined) of
                              time_range -> true;
                              time_point -> true;
                              _ -> false
                          end
                  end
              end,
              WrapMask,
              Specs),
            MinWidths1 = [min_width(S, W, Spec) || {S, W, Spec} <- lists:zip3(Stats, WrapMask1, Specs)],
            {WrapMask1, MinWidths1}
    end.

adjust_min_widths_for_details(Stats, Specs, WrapMask, MinWidths, TermWidth) ->
    Target = max(12, TermWidth div 3),
    zipwith4(
      fun(Stat, Spec, Wrap, MinW) ->
          case {Wrap, maps:get(role, Spec, undefined)} of
              {true, details} ->
                  MaxW = maps:get(max_width, Stat),
                  max(MinW, min(MaxW, Target));
              _ ->
                  MinW
          end
      end,
      Stats, Specs, WrapMask, MinWidths).

zipwith4(Fun, [A | As], [B | Bs], [C | Cs], [D | Ds]) ->
    [Fun(A, B, C, D) | zipwith4(Fun, As, Bs, Cs, Ds)];
zipwith4(_Fun, _A, _B, _C, _D) ->
    [].

column_priority_score(Stats, Idx, Count) ->
    Fill = maps:get(fill_count, Stats),
    Unique = maps:get(unique_count, Stats),
    MaxWidth = maps:get(max_width, Stats),
    OrderBoost = Count - Idx,
    Fill * 1000 + Unique - MaxWidth + OrderBoost.

column_priority_score(Stats, Spec, Idx, Count) ->
    Data = column_priority_score(Stats, Idx, Count) div 1000,
    Role = maps:get(role, Spec, undefined),
    Kind = maps:get(kind, Spec, undefined),
    OptionalPenalty = case maps:get(optional, Spec, false) of
        true -> -1000;
        false -> 0
    end,
    case maps:get(priority, Spec, undefined) of
        undefined ->
            role_priority(Role) + kind_priority(Kind) + Data + OptionalPenalty;
        Priority when is_integer(Priority) ->
            Priority + OptionalPenalty;
        _ ->
            role_priority(Role) + kind_priority(Kind) + Data + OptionalPenalty
    end.

flex_columns_from_stats(Headers, Stats, WrapMask) ->
    Candidates = lists:flatmap(
      fun({Idx, Stat, Wrap}) ->
          case Wrap of
              true -> [{Idx, maps:get(max_width, Stat)}];
              false -> []
          end
      end,
      lists:zip3(lists:seq(1, length(Stats)), Stats, WrapMask)),
    Sorted = lists:sort(fun({_, A}, {_, B}) -> A > B end, Candidates),
    [lists:nth(Idx, Headers) || {Idx, _} <- Sorted].

should_auto_split(Headers, Rows, Stats, Specs) ->
    SplitIdx = case type_column_index(Headers, Specs) of
        undefined -> label_index(Headers, "Type");
        FoundIdx -> FoundIdx
    end,
    case SplitIdx of
        undefined -> false;
        _ when length(Rows) =< 1 -> false;
        Idx ->
            Values = distinct_column_values(Rows, Idx),
            case length(Values) > 1 of
                false -> false;
                true ->
                    Total = length(Rows),
                    lists:any(
                      fun({ColIdx, Stat}) ->
                          ColIdx =/= Idx andalso maps:get(fill_count, Stat) < Total
                      end,
                      lists:zip(lists:seq(1, length(Stats)), Stats))
            end
    end.

distinct_column_values(Rows, Idx) ->
    Values = [string:trim(retinue_text:to_list(lists:nth(Idx, Row))) || Row <- Rows],
    lists:usort([V || V <- Values, V =/= "", V =/= "null"]).

list_to_index_map(List) ->
    maps:from_list(lists:zip(lists:seq(1, length(List)), List)).

wrap_widths(Headers, Rows, WrapMask, TermWidth, MinWidths, MaxWidths, Flex, Priorities, FlexColumns,
            Reflow, ReflowMax) ->
    ColumnCount = length(Headers),
    SepWidth = max(ColumnCount - 1, 0) * 2,
    Available = max(TermWidth - SepWidth, 0),
    BaseWidths = base_widths(Headers, Rows, WrapMask, MinWidths),
    case Flex of
        false -> BaseWidths;
        true ->
            DesiredWidths = desired_widths(Headers, Rows, WrapMask, MinWidths, MaxWidths),
            Widths0 = case lists:sum(DesiredWidths) =< Available of
                true -> DesiredWidths;
                false ->
                    Extra0 = Available - lists:sum(BaseWidths),
                    Extra = max(Extra0, 0),
                    {Widths1, Extra1} = distribute_growth(BaseWidths, DesiredWidths, WrapMask, Priorities, Extra),
                    distribute_flex_extra(Widths1, FlexColumns, Extra1)
            end,
            case Reflow of
                true -> reflow_widths(Widths0, Rows, WrapMask, MinWidths, MaxWidths, Priorities, FlexColumns, ReflowMax);
                false -> Widths0
            end
    end.

column_maxes_display(Rows, Count, Headers, WrapMask) ->
    HeaderLens = [retinue_tty:display_width(H) || H <- Headers],
    CellMaxes = column_maxes(Rows, Count),
    lists:zipwith3(
      fun(H, C, Wrap) ->
          case Wrap of
              true -> 0;
              false -> max(H, C)
          end
      end,
      HeaderLens, CellMaxes, pad_row(WrapMask, Count)).

column_maxes(Rows, Count) ->
    lists:foldl(
      fun(Row, Acc) ->
          RowVals = pad_row(Row, Count),
          lists:zipwith(fun(Cur, Val) -> max(Cur, retinue_tty:display_width(Val)) end, Acc, RowVals)
      end,
      lists:duplicate(Count, 0),
      Rows).

base_widths(Headers, Rows, WrapMask, MinWidths) ->
    ColumnCount = length(Headers),
    FixedWidths = column_maxes_display(Rows, ColumnCount, Headers, WrapMask),
    lists:map(
      fun({Wrap, MinW, Fixed}) ->
          case Wrap of
              true -> MinW;
              false -> Fixed
          end
      end,
      lists:zip3(pad_row(WrapMask, ColumnCount), pad_row(MinWidths, ColumnCount), FixedWidths)).

desired_widths(Headers, Rows, WrapMask, MinWidths, MaxWidths) ->
    ColumnCount = length(Headers),
    HeaderWidths = [retinue_tty:display_width(H) || H <- Headers],
    CellMaxes = column_maxes(Rows, ColumnCount),
    lists:map(
      fun(Idx) ->
          Wrap = lists:nth(Idx, pad_row(WrapMask, ColumnCount)),
          MinW = lists:nth(Idx, pad_row(MinWidths, ColumnCount)),
          MaxW = lists:nth(Idx, pad_row(MaxWidths, ColumnCount)),
          HeaderW = lists:nth(Idx, HeaderWidths),
          CellMax = lists:nth(Idx, CellMaxes),
          case Wrap of
              true ->
                  Target = max(MinW, max(HeaderW, CellMax)),
                  max(MinW, min(MaxW, Target));
              false ->
                  max(MinW, max(HeaderW, CellMax))
          end
      end,
      lists:seq(1, ColumnCount)).

expand_max_widths(MaxWidths, Count) when is_map(MaxWidths) ->
    lists:map(fun(Idx) -> maps:get(Idx, MaxWidths, 100000) end, lists:seq(1, Count));
expand_max_widths(_MaxWidths, Count) ->
    lists:duplicate(Count, 100000).

expand_min_widths(MinWidths, Count) when is_map(MinWidths) ->
    lists:map(fun(Idx) -> maps:get(Idx, MinWidths, 0) end, lists:seq(1, Count));
expand_min_widths(_MinWidths, Count) ->
    lists:duplicate(Count, 0).

default_priorities(Count) ->
    lists:reverse(lists:seq(1, Count)).

maybe_group_tables(Headers, Rows, Opts) ->
    case group_by_index(Headers, Opts) of
        undefined -> none;
        Idx ->
            RowMaps = maps:get(row_maps, Opts, undefined),
            Groups = case RowMaps of
                undefined -> group_rows(Rows, Idx);
                _ -> group_rows_with_maps(Rows, RowMaps, Idx)
            end,
            case Groups of
                [] -> none;
                _ ->
                    case length(Groups) > 1 of
                        false -> none;
                        true ->
                            LabelFun = maps:get(group_label_format, Opts, fun(Key) -> "== " ++ Key ++ " ==" end),
                            Opts1 = maps:without([group_by_column, group_by_label, auto_split_by_label, group_label_format], Opts),
                            Lines = grouped_lines(Headers, Groups, LabelFun, Opts1#{drop_column_index => Idx}),
                            {grouped, Lines}
                    end
            end
    end.

group_by_index(Headers, Opts) ->
    label_index(Headers, maps:get(auto_split_by_label, Opts, undefined)).

label_index(_Headers, undefined) -> undefined;
label_index(Headers, Label) ->
    Target = string:lowercase(retinue_text:to_list(Label)),
    case lists:dropwhile(
           fun({_Idx, Header}) ->
               string:lowercase(retinue_text:to_list(Header)) =/= Target
           end,
           lists:zip(lists:seq(1, length(Headers)), Headers)) of
        [] -> undefined;
        [{Idx, _} | _] -> Idx
    end.

group_rows(Rows, Idx) ->
    groups_from_rows(Rows, Idx, #{}, []).

groups_from_rows([], _Idx, _Seen, Acc) ->
    lists:reverse(Acc);
groups_from_rows([Row | Rest], Idx, Seen, Acc) ->
    Key = retinue_text:to_list(lists:nth(Idx, pad_row(Row, Idx))),
    case maps:get(Key, Seen, undefined) of
        undefined ->
            groups_from_rows(Rest, Idx, Seen#{Key => length(Acc) + 1}, Acc ++ [{Key, [Row]}]);
        Pos ->
            {Before, [{Key, Rows} | After]} = lists:split(Pos - 1, Acc),
            groups_from_rows(Rest, Idx, Seen, Before ++ [{Key, Rows ++ [Row]} | After])
    end.

group_rows_with_maps(Rows, RowMaps, Idx) ->
    groups_from_rows_with_maps(Rows, RowMaps, Idx, #{}, []).

groups_from_rows_with_maps([], _Maps, _Idx, _Seen, Acc) ->
    lists:reverse(Acc);
groups_from_rows_with_maps([Row | Rest], [Map | MapRest], Idx, Seen, Acc) ->
    Key = retinue_text:to_list(lists:nth(Idx, pad_row(Row, Idx))),
    case maps:get(Key, Seen, undefined) of
        undefined ->
            groups_from_rows_with_maps(Rest, MapRest, Idx, Seen#{Key => length(Acc) + 1},
                                       Acc ++ [{Key, [Row], [Map]}]);
        Pos ->
            {Before, [{Key, Rows, Maps} | After]} = lists:split(Pos - 1, Acc),
            groups_from_rows_with_maps(Rest, MapRest, Idx, Seen,
                                       Before ++ [{Key, Rows ++ [Row], Maps ++ [Map]} | After])
    end;
groups_from_rows_with_maps([_Row | Rest], [], Idx, Seen, Acc) ->
    groups_from_rows_with_maps(Rest, [], Idx, Seen, Acc).

grouped_lines(Headers, Groups, LabelFun, Opts) ->
    Resolver = maps:get(group_resolver, Opts, undefined),
    lists:flatmap(
      fun(Group) ->
          {Key, GroupRows, GroupMaps} = normalize_group(Group),
          Label = LabelFun(Key),
          Lines = case Resolver of
              Fun when is_function(Fun, 4) ->
                  {Headers1, Rows1, Opts1} = Fun(Key, GroupRows, GroupMaps, Opts),
                  case {Headers1, Rows1} of
                      {[], _} ->
                          render_lines_ungrouped(Headers, GroupRows, Opts);
                      _ ->
                          render_lines_ungrouped(Headers1, Rows1, Opts1)
                  end;
              _ ->
                  render_lines_ungrouped(Headers, GroupRows, Opts)
          end,
          [Label | Lines] ++ [""]
      end,
      Groups).

normalize_group({Key, Rows}) -> {Key, Rows, undefined};
normalize_group({Key, Rows, Maps}) -> {Key, Rows, Maps}.

maybe_drop_column_index(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required, Opts) ->
    case maps:get(drop_column_index, Opts, undefined) of
        Idx when is_integer(Idx, 1, length(Headers)) ->
            {
                drop_at(Headers, Idx),
                [drop_at(Row, Idx) || Row <- Rows],
                drop_at(WrapMask, Idx),
                drop_at(Priorities, Idx),
                drop_at(MinWidths, Idx),
                drop_at(MaxWidths, Idx),
                normalize_required(Required, Idx)
            };
        _ ->
            {Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required}
    end.

maybe_drop_empty_columns(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required, Opts) ->
    DropIdxs = empty_column_indexes(Headers, Rows, Required, Opts),
    {
        drop_indexes(Headers, DropIdxs),
        [drop_indexes(Row, DropIdxs) || Row <- Rows],
        drop_indexes(WrapMask, DropIdxs),
        drop_indexes(Priorities, DropIdxs),
        drop_indexes(MinWidths, DropIdxs),
        drop_indexes(MaxWidths, DropIdxs),
        normalize_required(Required, DropIdxs)
    }.

empty_column_indexes(Headers, Rows, _Required, Opts) ->
    lists:foldl(
      fun({Idx, _Header}, Acc) ->
          case column_empty(Rows, Idx, Headers, Opts) of
              true -> [Idx | Acc];
              false -> Acc
          end
      end,
      [],
      lists:zip(lists:seq(1, length(Headers)), Headers)).

column_empty(Rows, Idx, Headers, Opts) ->
    case column_part_values(Idx, Headers, Opts) of
        undefined ->
            lists:all(
              fun(Row) ->
                  Str = string:trim(retinue_text:to_list(lists:nth(Idx, Row))),
                  Str =:= "" orelse Str =:= "null"
              end,
              Rows);
        Values ->
            lists:all(fun(V) -> string:trim(retinue_text:to_list(V)) =:= "" end, Values)
    end.

maybe_drop_uniform_columns(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required, Opts) ->
    case length(Rows) =< 1 of
        true -> {Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required};
        false ->
            DropIdxs = uniform_drop_indexes(Headers, Rows, [], Opts),
            {
                drop_indexes(Headers, DropIdxs),
                [drop_indexes(Row, DropIdxs) || Row <- Rows],
                drop_indexes(WrapMask, DropIdxs),
                drop_indexes(Priorities, DropIdxs),
                drop_indexes(MinWidths, DropIdxs),
                drop_indexes(MaxWidths, DropIdxs),
                normalize_required(Required, DropIdxs)
            }
    end.

maybe_drop_uniform_columns_by_labels(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required, Opts) ->
    Labels = maps:get(drop_uniform_labels, Opts, []),
    case Labels of
        [] -> {Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required};
        _ ->
            DropIdxs = uniform_label_drop_indexes(Headers, Rows, Labels, Opts),
            {
                drop_indexes(Headers, DropIdxs),
                [drop_indexes(Row, DropIdxs) || Row <- Rows],
                drop_indexes(WrapMask, DropIdxs),
                drop_indexes(Priorities, DropIdxs),
                drop_indexes(MinWidths, DropIdxs),
                drop_indexes(MaxWidths, DropIdxs),
                normalize_required(Required, DropIdxs)
            }
    end.

uniform_drop_indexes(Headers, Rows, RequiredLabels, Opts) ->
    RequiredSet = required_label_set(RequiredLabels),
    lists:foldl(
      fun({Idx, Header}, Acc) ->
          case maps:is_key(normalize_label(Header), RequiredSet) of
              true -> Acc;
              false ->
                  case column_uniform(Rows, Idx, Headers, Opts) of
                      true -> [Idx | Acc];
                      false -> Acc
                  end
          end
      end,
      [],
      lists:zip(lists:seq(1, length(Headers)), Headers)).

uniform_label_drop_indexes(Headers, Rows, Labels, Opts) ->
    Targets = [normalize_label(L) || L <- Labels],
    lists:foldl(
      fun({Idx, Header}, Acc) ->
          case lists:member(normalize_label(Header), Targets) of
              false -> Acc;
              true ->
                  case column_uniform(Rows, Idx, Headers, Opts) of
                      true -> [Idx | Acc];
                      false -> Acc
                  end
          end
      end,
      [],
      lists:zip(lists:seq(1, length(Headers)), Headers)).

required_label_set(Labels) ->
    lists:foldl(
      fun(L, Acc) ->
          Acc#{normalize_label(L) => true}
      end,
      #{},
      Labels).

normalize_label(Label) ->
    string:lowercase(retinue_text:to_list(Label)).

column_uniform(Rows, Idx, Headers, Opts) ->
    Values = case column_part_values(Idx, Headers, Opts) of
        undefined -> [string:trim(retinue_text:to_list(lists:nth(Idx, Row))) || Row <- Rows];
        PartVals -> [string:trim(retinue_text:to_list(V)) || V <- PartVals]
    end,
    case Values of
        [] -> false;
        [First | Rest] ->
            First =/= "" andalso lists:all(fun(V) -> V =:= First end, Rest)
    end.

column_part_values(Idx, Headers, Opts) ->
    Specs = maps:get(column_specs, Opts, []),
    RowMaps = maps:get(row_maps, Opts, undefined),
    case RowMaps of
        undefined -> undefined;
        _ ->
            Header = lists:nth(Idx, Headers),
            Spec = find_spec_for_header(Header, Specs),
            Parts = parts_from_spec(Spec),
            case Parts of
                [_ | _] -> part_values(RowMaps, Parts);
                _ -> undefined
            end
    end.

parts_from_spec(Spec) ->
    case maps:get(source, Spec, undefined) of
        {row_map, Keys} -> Keys;
        _ -> maps:get(parts, Spec, undefined)
    end.

maybe_sort_sparse_columns(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required, Opts) ->
    _ = Opts,
    Order = sparse_column_order(Headers, Rows, Required),
    {
        reorder_by(Headers, Order),
        [reorder_by(Row, Order) || Row <- Rows],
        reorder_by(WrapMask, Order),
        reorder_by(Priorities, Order),
        reorder_by(MinWidths, Order),
        reorder_by(MaxWidths, Order),
        remap_required(Required, Order)
    }.

sparse_column_order(Headers, Rows, Required) ->
    ColCount = length(Headers),
    RowCount = length(Rows),
    FillCounts = [column_fill_count(Rows, Idx) || Idx <- lists:seq(1, ColCount)],
    DenseThreshold = case RowCount of
        0 -> 0;
        _ -> max(1, (RowCount + 1) div 2)
    end,
    RequiredSet = maps:from_list([{Idx, true} || Idx <- Required]),
    RequiredIdxs = [Idx || Idx <- lists:seq(1, ColCount), maps:is_key(Idx, RequiredSet)],
    DenseIdxs0 = [Idx || {Idx, Count} <- lists:zip(lists:seq(1, ColCount), FillCounts),
                         Count >= DenseThreshold, not maps:is_key(Idx, RequiredSet)],
    DenseSet = maps:from_list([{Idx, true} || Idx <- DenseIdxs0]),
    OptionalIdxs = [Idx || Idx <- lists:seq(1, ColCount),
                           not maps:is_key(Idx, RequiredSet),
                           not maps:is_key(Idx, DenseSet)],
    SortedOptional = lists:sort(
      fun(A, B) ->
          CA = lists:nth(A, FillCounts),
          CB = lists:nth(B, FillCounts),
          case CA =:= CB of
              true -> A < B;
              false -> CA > CB
          end
      end,
      OptionalIdxs),
    RequiredIdxs ++ DenseIdxs0 ++ SortedOptional.

column_fill_count(Rows, Idx) ->
    lists:sum(
      [case cell_present(lists:nth(Idx, Row)) of true -> 1; false -> 0 end || Row <- Rows]
    ).

reorder_by(List, Order) ->
    [lists:nth(Idx, List) || Idx <- Order].

remap_required(Required, Order) ->
    IndexMap = lists:foldl(
      fun({NewIdx, OldIdx}, Acc) -> Acc#{OldIdx => NewIdx} end,
      #{},
      lists:zip(lists:seq(1, length(Order)), Order)),
    lists:flatmap(
      fun(OldIdx) ->
          case maps:get(OldIdx, IndexMap, undefined) of
              undefined -> [];
              NewIdx -> [NewIdx]
          end
      end,
      Required).

tail_column_order(Headers, Rows, TailSpec) ->
    ColCount = length(Headers),
    TailIdxs0 = tail_column_indexes(Headers, TailSpec),
    TailIdxs = unique_indexes(TailIdxs0),
    FillCounts = [column_fill_count(Rows, Idx) || Idx <- lists:seq(1, ColCount)],
    RowCount = length(Rows),
    DenseThreshold = case RowCount of
        0 -> 0;
        _ -> max(1, (RowCount + 1) div 2)
    end,
    TailSparse = [Idx || {Idx, Count} <- lists:zip(lists:seq(1, ColCount), FillCounts),
                         lists:member(Idx, TailIdxs), Count < DenseThreshold],
    EmptyIdxs = [Idx || {Idx, Count} <- lists:zip(lists:seq(1, ColCount), FillCounts), Count =:= 0],
    RemainingFilled = [Idx || Idx <- lists:seq(1, ColCount),
                              not lists:member(Idx, TailSparse),
                              not lists:member(Idx, EmptyIdxs)],
    TailFilled = [Idx || Idx <- TailSparse, not lists:member(Idx, EmptyIdxs)],
    RemainingFilled ++ TailFilled ++ EmptyIdxs.

tail_column_indexes(_Headers, TailSpec) when is_integer(TailSpec) ->
    [TailSpec];
tail_column_indexes(Headers, Opts) when is_map(Opts) ->
    Tail = maps:get(tail_columns, Opts, []),
    TailKeys = [string:lowercase(retinue_text:to_list(Label)) || Label <- Tail],
    lists:foldl(
      fun({Idx, Header}, Acc) ->
          Key = string:lowercase(retinue_text:to_list(Header)),
          case lists:member(Key, TailKeys) of
              true -> [Idx | Acc];
              false -> Acc
          end
      end,
      [],
      lists:zip(lists:seq(1, length(Headers)), Headers));
tail_column_indexes(Headers, TailSpec) when is_list(TailSpec) ->
    lists:flatmap(
      fun(Col) ->
          case flex_column_index(Col, Headers) of
              undefined -> [];
              Idx -> [Idx]
          end
      end,
      TailSpec);
tail_column_indexes(_Headers, _TailSpec) ->
    [].

normalize_required(Required, DroppedIdxs) when is_list(DroppedIdxs) ->
    lists:foldl(fun(Idx, Acc) -> normalize_required(Acc, Idx) end, Required, lists:sort(DroppedIdxs));
normalize_required(Required, DroppedIdx) ->
    lists:foldl(
      fun(Idx, Acc) ->
          case Idx of
              _ when Idx =:= DroppedIdx -> Acc;
              _ when Idx > DroppedIdx -> [Idx - 1 | Acc];
              _ -> [Idx | Acc]
          end
      end,
      [],
      Required).

drop_indexes(List, DropIdxs) ->
    DropSet = maps:from_list([{I, true} || I <- DropIdxs]),
    [Val || {Idx, Val} <- lists:zip(lists:seq(1, length(List)), List), not maps:is_key(Idx, DropSet)].

distribute_growth(Widths, Desired, WrapMask, Priorities, Extra) ->
    Growable0 = [
        {Idx, lists:nth(Idx, Priorities)} ||
        {Idx, true} <- lists:zip(lists:seq(1, length(WrapMask)), WrapMask),
        lists:nth(Idx, Desired) > lists:nth(Idx, Widths)
    ],
    Groups = group_by_priority(Growable0),
    lists:foldl(
      fun({_, Idxs}, {CurWidths, CurExtra}) ->
          distribute_round_robin(CurWidths, Desired, Idxs, CurExtra)
      end,
      {Widths, Extra},
      Groups).

group_by_priority(Pairs) ->
    GroupMap = lists:foldl(
      fun({Idx, Priority}, Acc) ->
          maps:update_with(Priority, fun(Idxs) -> [Idx | Idxs] end, [Idx], Acc)
      end,
      #{},
      Pairs),
    SortedKeys = lists:reverse(lists:sort(maps:keys(GroupMap))),
    [{Key, lists:reverse(maps:get(Key, GroupMap))} || Key <- SortedKeys].

distribute_round_robin(Widths, Desired, Idxs, Extra) ->
    case Extra =< 0 of
        true -> {Widths, Extra};
        false ->
            case has_capacity(Widths, Desired, Idxs) of
                false -> {Widths, Extra};
                true ->
                    {Widths1, Extra1} = lists:foldl(
                      fun(Idx, {CurWidths, CurExtra}) ->
                          Cap = lists:nth(Idx, Desired) - lists:nth(Idx, CurWidths),
                          case CurExtra > 0 andalso Cap > 0 of
                              true -> {add_at(CurWidths, Idx, 1), CurExtra - 1};
                              false -> {CurWidths, CurExtra}
                          end
                      end,
                      {Widths, Extra},
                      Idxs),
                    distribute_round_robin(Widths1, Desired, Idxs, Extra1)
            end
    end.

has_capacity(Widths, Desired, Idxs) ->
    lists:any(fun(Idx) -> lists:nth(Idx, Desired) > lists:nth(Idx, Widths) end, Idxs).

distribute_flex_extra(Widths, FlexColumns, Extra) ->
    case Extra =< 0 of
        true -> Widths;
        false ->
            case FlexColumns of
                [] -> Widths;
                _ -> distribute_flex_round_robin(Widths, FlexColumns, Extra)
            end
    end.

distribute_flex_round_robin(Widths, FlexColumns, Extra) ->
    case Extra =< 0 of
        true -> Widths;
        false ->
            {Widths1, Extra1} = lists:foldl(
              fun(Idx, {CurWidths, CurExtra}) ->
                  case CurExtra > 0 of
                      true -> {add_at(CurWidths, Idx, 1), CurExtra - 1};
                      false -> {CurWidths, CurExtra}
                  end
              end,
              {Widths, Extra},
              FlexColumns),
            distribute_flex_round_robin(Widths1, FlexColumns, Extra1)
    end.

reflow_widths(Widths, Rows, WrapMask, MinWidths, MaxWidths, Priorities, FlexColumns, MaxIter) ->
    reflow_widths_loop(Widths, Rows, WrapMask, MinWidths, MaxWidths, Priorities, FlexColumns, MaxIter, 0).

reflow_widths_loop(Widths, _Rows, _WrapMask, _MinWidths, _MaxWidths, _Priorities, _FlexColumns, MaxIter, MaxIter) ->
    Widths;
reflow_widths_loop(Widths, Rows, WrapMask, MinWidths, MaxWidths, Priorities, FlexColumns, MaxIter, Iter) ->
    case best_reflow_shift(Widths, Rows, WrapMask, MinWidths, MaxWidths, Priorities, FlexColumns) of
        none -> Widths;
        {NewWidths, improved} -> reflow_widths_loop(NewWidths, Rows, WrapMask, MinWidths, MaxWidths,
                                                    Priorities, FlexColumns, MaxIter, Iter + 1)
    end.

best_reflow_shift(_Widths, _Rows, _WrapMask, _MinWidths, _MaxWidths, _Priorities, []) ->
    none;
best_reflow_shift(Widths, Rows, WrapMask, MinWidths, MaxWidths, Priorities, FlexColumns) ->
    BaseHeight = total_height(Rows, Widths, WrapMask),
    Shrink = shrink_candidates(Widths, MinWidths, WrapMask, FlexColumns),
    Expand = expand_candidates(Widths, MaxWidths, FlexColumns),
    case {Shrink, Expand} of
        {[], _} -> none;
        {_, []} -> none;
        _ ->
            Best0 = {undefined, undefined, BaseHeight},
            Best = lists:foldl(
              fun(From, Acc) ->
                  lists:foldl(
                    fun(To, Acc1) ->
                        Widths1 = add_at(add_at(Widths, From, -1), To, 1),
                        Height1 = total_height(Rows, Widths1, WrapMask),
                        choose_better_shift({From, To, Height1}, Acc1, Priorities)
                    end,
                    Acc,
                    Expand)
              end,
              Best0,
              Shrink),
            case Best of
                {undefined, undefined, _} -> none;
                {From, To, Height1} when Height1 =< BaseHeight ->
                    {add_at(add_at(Widths, From, -1), To, 1), improved};
                _ -> none
            end
    end.

choose_better_shift({From, To, Height}, {BestFrom, BestTo, BestHeight}, Priorities) ->
    case BestFrom of
        undefined -> {From, To, Height};
        _ ->
            case Height < BestHeight of
                true -> {From, To, Height};
                false when Height =:= BestHeight ->
                    PTo = lists:nth(To, Priorities),
                    PBestTo = lists:nth(BestTo, Priorities),
                    case PTo > PBestTo of
                        true -> {From, To, Height};
                        false when PTo =:= PBestTo ->
                            PFrom = lists:nth(From, Priorities),
                            PBestFrom = lists:nth(BestFrom, Priorities),
                            case PFrom < PBestFrom of
                                true -> {From, To, Height};
                                false -> {BestFrom, BestTo, BestHeight}
                            end;
                        false -> {BestFrom, BestTo, BestHeight}
                    end;
                false -> {BestFrom, BestTo, BestHeight}
            end
    end.

shrink_candidates(Widths, MinWidths, WrapMask, FlexColumns) ->
    FlexSet = maps:from_list([{I, true} || I <- FlexColumns]),
    lists:flatmap(
      fun({Idx, {Width, MinW, Wrap}}) ->
          case Wrap =:= true andalso Width > MinW andalso not maps:is_key(Idx, FlexSet) of
              true -> [Idx];
              false -> []
          end
      end,
      lists:zip(lists:seq(1, length(Widths)), lists:zip3(Widths, MinWidths, WrapMask))).

expand_candidates(Widths, MaxWidths, FlexColumns) ->
    lists:flatmap(
      fun(Idx) ->
          case lists:nth(Idx, Widths) < lists:nth(Idx, MaxWidths) of
              true -> [Idx];
              false -> []
          end
      end,
      FlexColumns).

total_height(Rows, Widths, WrapMask) ->
    lists:sum([row_height(Row, Widths, WrapMask) || Row <- Rows]).

row_height(Row, Widths, WrapMask) ->
    Cells = pad_row(Row, length(Widths)),
    Wrapped = lists:zipwith3(fun retinue_table_text:wrap_cell/3,
                             Cells, Widths, WrapMask),
    lists:max([length(Cell) || Cell <- Wrapped] ++ [1]).

add_at(Widths, Idx, Delta) ->
    lists:map(
      fun({I, W}) ->
          case I =:= Idx of
              true -> W + Delta;
              false -> W
          end
      end,
      lists:zip(lists:seq(1, length(Widths)), Widths)).

apply_layout_rules(State, Opts, Context) ->
    lists:foldl(
      fun(Rule, Acc) -> apply_layout_rule(Rule, Acc, Opts, Context) end,
      State,
      layout_rules(Opts)).

layout_rules(Opts) ->
    _ = Opts,
    [
        drop_column_index,
        drop_empty_columns,
        drop_uniform_columns,
        drop_columns,
        sort_sparse_columns,
        tail_columns
    ].

apply_layout_rule(drop_column_index, State, Opts, _Context) ->
    apply_table_transform(fun maybe_drop_column_index/8, State, Opts);
apply_layout_rule(drop_empty_columns, State, Opts, _Context) ->
    apply_table_transform(fun maybe_drop_empty_columns/8, State, Opts);
apply_layout_rule(drop_uniform_columns, State, Opts, Context) ->
    Headers = maps:get(headers, State),
    MinWidths = maps:get(min_widths, State),
    TermWidth = maps:get(term_width, Context),
    Cols = length(Headers),
    Sep = case Cols of
        0 -> 0;
        _ -> (Cols - 1) * 2
    end,
    Total = lists:sum(MinWidths) + Sep,
    State1 = apply_table_transform(fun maybe_drop_uniform_columns_by_labels/8, State, Opts),
    case Total =< TermWidth of
        true -> State1;
        false -> apply_table_transform(fun maybe_drop_uniform_columns/8, State1, Opts)
    end;
apply_layout_rule(drop_columns, State, _Opts, Context) ->
    Headers = maps:get(headers, State),
    Rows = maps:get(rows, State),
    WrapMask = maps:get(wrap_mask, State),
    Priorities = maps:get(priorities, State),
    MinWidths = maps:get(min_widths, State),
    MaxWidths = maps:get(max_widths, State),
    Required = maps:get(required, State),
    TermWidth = maps:get(term_width, Context),
    DropColumns = maps:get(drop_columns, Context),
    Mode = maps:get(mode, Context),
    {Headers1, Rows1, WrapMask1, Priorities1, MinWidths1, MaxWidths1, Required1} =
        maybe_drop_columns(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths,
                           TermWidth, DropColumns, Mode, Required),
    State#{
        headers => Headers1,
        rows => Rows1,
        wrap_mask => WrapMask1,
        priorities => Priorities1,
        min_widths => MinWidths1,
        max_widths => MaxWidths1,
        required => Required1
    };
apply_layout_rule(sort_sparse_columns, State, Opts, _Context) ->
    Headers = maps:get(headers, State),
    Rows = maps:get(rows, State),
    WrapMask = maps:get(wrap_mask, State),
    Priorities = maps:get(priorities, State),
    MinWidths = maps:get(min_widths, State),
    MaxWidths = maps:get(max_widths, State),
    Required = maps:get(required, State),
    {Headers1, Rows1, WrapMask1, Priorities1, MinWidths1, MaxWidths1, Required1} =
        maybe_sort_sparse_columns(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required, Opts),
    State#{
        headers => Headers1,
        rows => Rows1,
        wrap_mask => WrapMask1,
        priorities => Priorities1,
        min_widths => MinWidths1,
        max_widths => MaxWidths1,
        required => Required1
    };
apply_layout_rule(tail_columns, State, Opts, _Context) ->
    case maps:get(tail_columns, Opts, undefined) of
        undefined -> State;
        [] -> State;
        TailSpec ->
            Headers = maps:get(headers, State),
            Rows = maps:get(rows, State),
            WrapMask = maps:get(wrap_mask, State),
            Priorities = maps:get(priorities, State),
            MinWidths = maps:get(min_widths, State),
            MaxWidths = maps:get(max_widths, State),
            Required = maps:get(required, State),
            Order = tail_column_order(Headers, Rows, TailSpec),
            State#{
                headers => reorder_by(Headers, Order),
                rows => [reorder_by(Row, Order) || Row <- Rows],
                wrap_mask => reorder_by(WrapMask, Order),
                priorities => reorder_by(Priorities, Order),
                min_widths => reorder_by(MinWidths, Order),
                max_widths => reorder_by(MaxWidths, Order),
                required => remap_required(Required, Order)
            }
    end;
apply_layout_rule(_Other, State, _Opts, _Context) ->
    State.

apply_table_transform(Fun, State, Opts) ->
    Headers = maps:get(headers, State),
    Rows = maps:get(rows, State),
    WrapMask = maps:get(wrap_mask, State),
    Priorities = maps:get(priorities, State),
    MinWidths = maps:get(min_widths, State),
    MaxWidths = maps:get(max_widths, State),
    Required = maps:get(required, State),
    {Headers1, Rows1, WrapMask1, Priorities1, MinWidths1, MaxWidths1, Required1} =
        Fun(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required, Opts),
    State#{
        headers => Headers1,
        rows => Rows1,
        wrap_mask => WrapMask1,
        priorities => Priorities1,
        min_widths => MinWidths1,
        max_widths => MaxWidths1,
        required => Required1
    }.

normalize_flex_columns(Columns, Headers, WrapMask, Priorities) ->
    ColIdxs0 = lists:flatmap(
      fun(Col) ->
          case flex_column_index(Col, Headers) of
              undefined -> [];
              Idx -> [Idx]
          end
      end,
      Columns),
    ColIdxs1 = unique_indexes([Idx || Idx <- ColIdxs0, is_flex_candidate(Idx, WrapMask)]),
    case ColIdxs1 of
        [] -> default_flex_columns(WrapMask, Priorities);
        _ -> ColIdxs1
    end.

flex_column_index(Idx, _Headers) when is_integer(Idx) -> Idx;
flex_column_index(Col, Headers) ->
    Label = string:lowercase(retinue_text:to_list(Col)),
    case lists:zip(lists:seq(1, length(Headers)), Headers) of
        [] -> undefined;
        Pairs ->
            case lists:dropwhile(
                   fun({_Idx, Header}) ->
                       string:lowercase(retinue_text:to_list(Header)) =/= Label
                   end,
                   Pairs) of
                [] -> undefined;
                [{Idx, _} | _] -> Idx
            end
    end.

unique_indexes(Idxs) ->
    lists:reverse(lists:foldl(fun(I, Acc) -> case lists:member(I, Acc) of true -> Acc; false -> [I | Acc] end end, [], Idxs)).

is_flex_candidate(Idx, WrapMask) when Idx =< length(WrapMask) ->
    lists:nth(Idx, WrapMask) =:= true;
is_flex_candidate(_Idx, _WrapMask) ->
    false.

default_flex_columns(WrapMask, Priorities) ->
    WrapIdxs = [Idx || {Idx, true} <- lists:zip(lists:seq(1, length(WrapMask)), WrapMask)],
    case WrapIdxs of
        [] -> [];
        _ ->
            [{Idx, _} | _] = lists:sort(
                fun({_, P1}, {_, P2}) -> P1 > P2 end,
                [{Idx, lists:nth(Idx, Priorities)} || Idx <- WrapIdxs]),
            [Idx]
    end.

maybe_drop_columns(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, TermWidth, Drop, Mode, Required) ->
    case Drop andalso Mode =/= none of
        false -> {Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required};
        true -> drop_columns_loop(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, TermWidth, Required)
    end.

drop_columns_loop(Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, TermWidth, Required) ->
    Cols = length(Headers),
    Sep = case Cols of
        0 -> 0;
        _ -> (Cols - 1) * 2
    end,
    Total = lists:sum(MinWidths) + Sep,
    case Total =< TermWidth orelse Cols =< 1 of
        true -> {Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required};
        false ->
            Idx = lowest_priority_index(Priorities, Required),
            case Idx of
                none -> {Headers, Rows, WrapMask, Priorities, MinWidths, MaxWidths, Required};
                _ ->
                    Headers1 = drop_at(Headers, Idx),
                    Rows1 = [drop_at(R, Idx) || R <- Rows],
                    WrapMask1 = drop_at(WrapMask, Idx),
                    Priorities1 = drop_at(Priorities, Idx),
                    MinWidths1 = drop_at(MinWidths, Idx),
                    MaxWidths1 = drop_at(MaxWidths, Idx),
                    Required1 = normalize_required(Required, Idx),
                    drop_columns_loop(Headers1, Rows1, WrapMask1, Priorities1, MinWidths1, MaxWidths1, TermWidth, Required1)
            end
    end.

lowest_priority_index([], _Required) -> none;
lowest_priority_index(Priorities, Required) ->
    RequiredSet = maps:from_list([{I, true} || I <- Required]),
    Candidates = [Pair || Pair = {I, _} <- lists:zip(lists:seq(1, length(Priorities)), Priorities),
                          not maps:is_key(I, RequiredSet)],
    case Candidates of
        [] -> none;
        _ ->
            {Idx, _} =
                lists:foldl(
                  fun({I, P}, {BestI, BestP}) ->
                      case P < BestP orelse (P =:= BestP andalso I > BestI) of
                          true -> {I, P};
                          false -> {BestI, BestP}
                      end
                  end,
                  hd(Candidates),
                  tl(Candidates)),
            Idx
    end.

drop_at(List, Idx) ->
    {Head, Tail} = lists:split(Idx - 1, List),
    case Tail of
        [] -> List;
        [_ | Rest] -> Head ++ Rest
    end.

normalize_column_specs(Headers, Opts) ->
    Specs0 = maps:get(column_specs, Opts, []),
    Intent = maps:get(intent, Opts, #{}),
    Specs1 = apply_intent_specs(Specs0, Intent),
    [find_spec_for_header(Header, Specs1) || Header <- Headers].

apply_intent_specs(Specs, Intent) ->
    Priority = maps:get(priority, Intent, #{}),
    Optional = maps:get(optional, Intent, #{}),
    lists:map(fun(Spec) -> apply_intent_spec(Spec, Priority, Optional) end, Specs).

apply_intent_spec(Spec0, Priority, Optional) ->
    Key = maps:get(key, Spec0, undefined),
    Spec1 = case maps:is_key(Key, Priority) of
        true -> Spec0#{priority => maps:get(Key, Priority)};
        false -> Spec0
    end,
    case maps:is_key(Key, Optional) of
        true -> Spec1#{optional => maps:get(Key, Optional)};
        false -> Spec1
    end.

find_spec_for_header(Header, Specs) ->
    Key = string:lowercase(retinue_text:to_list(Header)),
    case lists:dropwhile(
           fun(Spec) ->
               Label = maps:get(label, Spec, ""),
               string:lowercase(retinue_text:to_list(Label)) =/= Key
           end,
           Specs) of
        [] -> #{};
        [Spec | _] -> Spec
    end.

wrap_for_spec(Spec) ->
    case maps:get(wrap, Spec, undefined) of
        true -> true;
        false -> false;
        _ ->
            Role = maps:get(role, Spec, undefined),
            Kind = maps:get(kind, Spec, undefined),
            case Role of
                details -> true;
                link -> true;
                flags -> true;
                extra -> true;
                _ ->
                    case Kind of
                        time_range -> false;
                        time_point -> false;
                        _ -> false
                    end
            end
    end.

role_priority(undefined) -> 5000;
role_priority(name) -> 8500;
role_priority(id) -> 9000;
role_priority(time) -> 8000;
role_priority(type) -> 7500;
role_priority(location) -> 7000;
role_priority(mission) -> 6800;
role_priority(stat) -> 6000;
role_priority(details) -> 7200;
role_priority(link) -> 3500;
role_priority(flags) -> 3200;
role_priority(extra) -> 2000;
role_priority(_) -> 5000.

kind_priority(time_point) -> 800;
kind_priority(time_range) -> 1000;
kind_priority(numeric) -> 600;
kind_priority(enum) -> 400;
kind_priority(path) -> 200;
kind_priority(_) -> 0.

type_column_index(Headers, Specs) ->
    case Specs of
        [] -> undefined;
        _ ->
            case lists:dropwhile(
                   fun({_Idx, Spec}) ->
                       maps:get(role, Spec, undefined) =/= type
                   end,
                   lists:zip(lists:seq(1, length(Headers)), Specs)) of
                [] -> undefined;
                [{Idx, _} | _] -> Idx
            end
    end.

drop_uniform_labels_from_specs(Specs, Opts) ->
    Default = ["Type"],
    case maps:get(drop_uniform_labels, Opts, undefined) of
        undefined ->
            SpecLabels = [
                maps:get(label, Spec)
                || Spec <- Specs, maps:get(role, Spec, undefined) =:= type,
                   maps:get(label, Spec, undefined) =/= undefined
            ],
            case SpecLabels of
                [] -> Default;
                _ -> SpecLabels
            end;
        Labels -> Labels
    end.

tail_columns_from_specs(Specs, Opts) ->
    case maps:get(tail_columns, Opts, undefined) of
        undefined ->
            [
                Label
                || Spec <- Specs,
                   lists:member(maps:get(role, Spec, undefined), [details, link, flags, extra]),
                   Label <- [maps:get(label, Spec, undefined)],
                   Label =/= undefined
            ];
        Tail -> Tail
    end.

flex_columns_from_specs(Headers, Specs, WrapMask, Stats) ->
    SpecFlex = [
        maps:get(label, Spec)
        || Spec <- Specs,
           lists:member(maps:get(role, Spec, undefined), [details, link, flags]),
           maps:get(label, Spec, undefined) =/= undefined
    ],
    FromStats = flex_columns_from_stats(Headers, Stats, WrapMask),
    SpecFlex ++ FromStats.

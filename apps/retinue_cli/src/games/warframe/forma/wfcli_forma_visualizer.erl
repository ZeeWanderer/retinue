%%%-------------------------------------------------------------------
%% Forma plan HTML and SVG rendering.
%%%-------------------------------------------------------------------
-module(wfcli_forma_visualizer).

-export([show_plan/4, show_config_plan/4, render_html/5, render_svg/5, open_file/1,
         render_config_html/5, render_config_svg/5]).

-type path() :: file:filename_all().
-type plan() :: map().
-type slot_mod_labels() :: [{term(), [{term(), term()}]}].
-type build_arcanes() :: [{term(), [term()]}].
-type render_result() :: {ok, path()} | {error, term()}.

-doc "Render a solved Forma plan to its default HTML path.".
-spec show_plan(path(), plan(), slot_mod_labels(), build_arcanes()) -> ok | term().
show_plan(File, Plan, SlotMods, BuildArcanes) ->
    show_html(File, Plan, SlotMods, BuildArcanes, fun render_html/5).

-doc "Render current config polarities to their default HTML path.".
-spec show_config_plan(path(), plan(), slot_mod_labels(), build_arcanes()) -> ok | term().
show_config_plan(File, Plan, SlotMods, BuildArcanes) ->
    show_html(File, Plan, SlotMods, BuildArcanes, fun render_config_html/5).

show_html(File, Plan, SlotMods, BuildArcanes, Renderer) ->
    case Renderer(File, Plan, SlotMods, BuildArcanes, undefined) of
        {ok, Path} ->
            io:format("visualization (html) written to ~s~n", [Path]),
            ok;
        {error, Reason} ->
            io:format("visualization html write failed: ~p~n", [Reason]),
            {error, Reason}
    end.

find_mods(Slot, SlotMods) ->
    case lists:keyfind(Slot, 1, SlotMods) of
        false -> [];
        {_, Mods} ->
            [{wfcli_forma_plan:to_list(B), wfcli_forma_plan:to_list(M)} || {B, M} <- Mods]
    end.

-doc "Write a plan visualization as HTML and return `{ok, Path}` or `{error, Reason}`.".
-spec render_html(path(), plan(), slot_mod_labels(), build_arcanes(), path() | undefined) -> render_result().
render_html(File, Plan, SlotMods, BuildArcanes, undefined) ->
    render_html(File, Plan, SlotMods, BuildArcanes, default_viz_path(File, ".plan.html"));
render_html(File, Plan, SlotMods, BuildArcanes, Target) ->
    Iolist = build_html(File, Plan, SlotMods, BuildArcanes),
    Bin = iolist_to_binary(Iolist),
    case file:write_file(Target, Bin) of
        ok -> {ok, Target};
        Error -> {error, Error}
    end.

-doc "Write a plan visualization as SVG and return `{ok, Path}` or `{error, Reason}`.".
-spec render_svg(path(), plan(), slot_mod_labels(), build_arcanes(), path() | undefined) -> render_result().
render_svg(File, Plan, SlotMods, BuildArcanes, undefined) ->
    render_svg(File, Plan, SlotMods, BuildArcanes, default_viz_path(File, ".plan.svg"));
render_svg(File, Plan, SlotMods, BuildArcanes, Target) ->
    Iolist = build_svg(File, Plan, SlotMods, BuildArcanes),
    Bin = iolist_to_binary(Iolist),
    case file:write_file(Target, Bin) of
        ok -> {ok, Target};
        Error -> {error, Error}
    end.

-doc "Write current-config polarities as HTML instead of solved-plan output.".
-spec render_config_html(path(), plan(), slot_mod_labels(), build_arcanes(), path() | undefined) -> render_result().
render_config_html(File, Plan, SlotMods, BuildArcanes, undefined) ->
    render_html(File, Plan, SlotMods, BuildArcanes, default_viz_path(File, ".config.html"));
render_config_html(File, Plan, SlotMods, BuildArcanes, Target) ->
    render_html(File, Plan, SlotMods, BuildArcanes, Target).

-doc "Write current-config polarities as SVG instead of solved-plan output.".
-spec render_config_svg(path(), plan(), slot_mod_labels(), build_arcanes(), path() | undefined) -> render_result().
render_config_svg(File, Plan, SlotMods, BuildArcanes, undefined) ->
    render_svg(File, Plan, SlotMods, BuildArcanes, default_viz_path(File, ".config.svg"));
render_config_svg(File, Plan, SlotMods, BuildArcanes, Target) ->
    render_svg(File, Plan, SlotMods, BuildArcanes, Target).

default_viz_path(File, Suffix) ->
    Path = wfcli_forma_plan:to_list(File),
    filename:absname(filename:join(filename:dirname(Path), default_viz_base(Path) ++ Suffix)).

default_viz_base(File) ->
    Root = filename:rootname(filename:basename(File)),
    strip_suffix(Root, ".plan").

strip_suffix(Text, Suffix) ->
    TextLen = length(Text),
    SuffixLen = length(Suffix),
    case TextLen > SuffixLen andalso lists:suffix(Suffix, Text) of
        true -> lists:sublist(Text, TextLen - SuffixLen);
        false -> Text
    end.

-doc "Open a rendered file through the platform opener without invoking a shell.".
-spec open_file(path() | term()) -> ok.
open_file(Path) when is_list(Path) ->
    case open_file_command(Path) of
        {ok, Executable, Args} ->
            _ = open_port({spawn_executable, Executable}, [{args, Args}]),
            ok;
        skip ->
            ok
    end;
open_file(_) -> ok.

open_file_command(Path) ->
    case os:type() of
        {unix, _} ->
            case os:find_executable("xdg-open") of
                false -> skip;
                XdgOpen -> {ok, XdgOpen, [Path]}
            end;
        {win32, _} ->
            case os:find_executable("cmd") of
                false -> skip;
                Cmd -> {ok, Cmd, ["/c", "start", "", Path]}
            end;
        _ ->
            skip
    end.

build_html(File, Plan, SlotMods, BuildArcanes) ->
    Slots = sort_plan(Plan),
    SlotMap = maps:from_list(Slots),
    AuraRow = pad_row([{aura, maps:get(aura, SlotMap, none)}, {exilus, maps:get(exilus, SlotMap, none)}], 2),
    Normal = [{S, maps:get(S, SlotMap, none)} || {S, _} <- Slots, is_integer(S)],
    {Row1, Row2} = normal_rows(Normal),
    Row1P = pad_row(Row1, 4),
    Row2P = pad_row(Row2, 4),
    ArcaneHtml = build_arcanes_html(BuildArcanes),
    [
      "<!doctype html><html><head><style>",
      "body{font-family:sans-serif;background:#0b1117;color:#e6edf3;padding:16px;}",
      ".grid2{display:grid;grid-template-columns:repeat(2,220px);grid-gap:12px;margin-bottom:12px;}",
      ".grid4{display:grid;grid-template-columns:repeat(4,220px);grid-gap:12px;}",
      ".card{background:#121a22;border:1px solid #1f2b36;border-radius:8px;padding:10px;}",
      ".title{font-weight:bold;margin:0 0 4px 0;}",
      ".pol{color:#8bd5ff;margin:0 0 6px 0;}",
      ".mods{margin:0;padding-left:16px;}",
      ".arcanes{margin:12px 0 18px 0;padding:10px;border:1px solid #1f2b36;border-radius:8px;background:#10161d;}",
      ".arcanes h3{margin:0 0 6px 0;font-size:14px;}",
      ".arcanes ul{margin:0;padding-left:16px;}",
      "</style></head><body>",
      "<h2>Forma Plan: ", File, "</h2>",
      ArcaneHtml,
      "<div class='grid2'>", card_html(AuraRow, SlotMods), "</div>",
      "<div class='grid4'>", card_html(Row1P, SlotMods), "</div>",
      "<div class='grid4'>", card_html(Row2P, SlotMods), "</div>",
      "</body></html>"
    ].

build_arcanes_html(BuildArcanes) ->
    Lines = arcane_lines(BuildArcanes),
    case Lines of
        [] -> "";
        _ ->
            ["<div class='arcanes'><h3>Arcanes</h3><ul>",
             [ ["<li>", Line, "</li>"] || Line <- Lines ],
             "</ul></div>"]
    end.

build_svg(File, Plan, SlotMods, BuildArcanes) ->
    Slots = sort_plan(Plan),
    SlotMap = maps:from_list(Slots),
    AuraRow = pad_row([{aura, maps:get(aura, SlotMap, none)}, {exilus, maps:get(exilus, SlotMap, none)}], 2),
    Normal = [{S, maps:get(S, SlotMap, none)} || {S, _} <- Slots, is_integer(S)],
    {Row1, Row2} = normal_rows(Normal),
    Row1P = pad_row(Row1, 4),
    Row2P = pad_row(Row2, 4),
    CardW = 220, CardH = 120, Gap = 12,
    Width = ((max(length(Row1P), length(Row2P)) + 2) * (CardW + Gap)),
    {ArcaneSvg, GridYOffset} = build_arcanes_svg(BuildArcanes),
    AuraYOffset = GridYOffset,
    Row1YOffset = GridYOffset + CardH + Gap,
    Row2YOffset = GridYOffset + (CardH + Gap) * 2,
    Height = GridYOffset + (CardH * 3) + (Gap * 2) + 20,
    [
      "<svg xmlns='http://www.w3.org/2000/svg' width='", integer_to_list(Width),
      "' height='", integer_to_list(Height), "' style='background:#0b1117;font-family:sans-serif'>",
      "<text x='10' y='20' fill='#e6edf3' font-size='14'>", File, "</text>",
      ArcaneSvg,
      svg_row(AuraRow, SlotMods, CardW, CardH, Gap, AuraYOffset, 2),
      svg_row(Row1P, SlotMods, CardW, CardH, Gap, Row1YOffset, 4),
      svg_row(Row2P, SlotMods, CardW, CardH, Gap, Row2YOffset, 4),
      "</svg>"
    ].

build_arcanes_svg(BuildArcanes) ->
    Lines = arcane_lines(BuildArcanes),
    case Lines of
        [] -> {[], 0};
        _ ->
            StartY = 40,
            LineHeight = 16,
            SvgLines =
                [ ["<text x='10' y='", integer_to_list(StartY + (Idx * LineHeight)),
                    "' fill='#cbd4de' font-size='12'>", Line, "</text>"]
                  || {Idx, Line} <- lists:zip(lists:seq(0, length(Lines) - 1), Lines) ],
            GridYOffset = StartY + (length(Lines) * LineHeight) + 12,
            {SvgLines, GridYOffset}
    end.

card_html(Pairs, SlotMods) ->
    [ card_html_slot(Slot, Pol, SlotMods) || {Slot, Pol} <- Pairs ].

card_html_slot(none, _Pol, _SlotMods) -> "";
card_html_slot(Slot, Pol, SlotMods) ->
    PolRaw = wfcli_polarity:symbol(Pol),
    PolStr = case PolRaw of null -> "none"; _ -> wfcli_forma_plan:to_list(PolRaw) end,
    Mods = find_mods(Slot, SlotMods),
    [
      "<div class='card'>",
      "<div class='title'>Slot ", io_lib:format("~p", [Slot]), "</div>",
      "<div class='pol'>Polarity: ", PolStr, "</div>",
      "<ul class='mods'>",
      [ ["<li>", Build, ": ", Mod, "</li>"] || {Build, Mod} <- Mods ],
      "</ul></div>"
    ].

normal_rows(Normal) ->
    Padded = Normal ++ lists:duplicate(max(0, 8 - length(Normal)), {none, none}),
    {Row1, Rest} = lists:split(4, Padded),
    {Row2, _} = lists:split(4, Rest),
    {Row1, Row2}.

pad_row(Row, N) ->
    Row ++ lists:duplicate(max(0, N - length(Row)), {none, none}).

svg_row(Pairs, SlotMods, CardW, CardH, Gap, YOffset, Columns) ->
    lists:flatten(
      [svg_card(Index, Slot, Pol, SlotMods, CardW, CardH, Gap, YOffset, Columns) || {Index, {Slot, Pol}} <- lists:zip(lists:seq(0, length(Pairs)-1), Pairs)] ).

svg_card(_Index, none, _Pol, _SlotMods, _W, _H, _Gap, _Y, _Cols) -> [];
svg_card(Index, Slot, Pol, SlotMods, W, H, Gap, YOffset, Columns) ->
    Col = Index rem Columns,
    X = Col * (W + Gap),
    PolRaw = wfcli_polarity:symbol(Pol),
    PolStr = case PolRaw of null -> "none"; _ -> wfcli_forma_plan:to_list(PolRaw) end,
    Mods = find_mods(Slot, SlotMods),
    [
      "<g transform='translate(", integer_to_list(X), ",", integer_to_list(YOffset), ")'>",
      "<rect x='0' y='0' rx='8' ry='8' width='", integer_to_list(W),
      "' height='", integer_to_list(H), "' fill='#121a22' stroke='#1f2b36'/>",
      "<text x='10' y='18' fill='#e6edf3' font-size='12'>Slot ", wfcli_forma_plan:to_list(Slot), "</text>",
      "<text x='10' y='36' fill='#8bd5ff' font-size='12'>Polarity: ", PolStr, "</text>",
      svg_mods(Mods, 52),
      "</g>"
    ].

svg_mods(Mods, StartY) ->
    lists:flatten(
      [ ["<text x='12' y='", integer_to_list(StartY + (Idx * 16)),
          "' fill='#cbd4de' font-size='11'>- ", Build, ": ", Mod, "</text>"]
        || {Idx, {Build, Mod}} <- lists:zip(lists:seq(0, length(Mods)-1), Mods)
      ]).

arcane_lines(BuildArcanes) ->
    LinesRev =
        lists:foldl(
          fun({Build, Arcanes}, Acc) ->
              Labels = arcane_labels(Arcanes),
              case Labels of
                  [] -> Acc;
                  _ ->
                      Line = lists:flatten(io_lib:format("~s: ~s", [wfcli_forma_plan:to_list(Build), string:join(Labels, ", ")])),
                      [Line | Acc]
              end
          end,
          [],
          BuildArcanes),
    lists:reverse(LinesRev).

arcane_labels(Arcanes) when is_list(Arcanes) ->
    [Label || Label <- [arcane_label(Arcane) || Arcane <- Arcanes], Label =/= ""];
arcane_labels(_) ->
    [].

arcane_label(#{name := Name} = Arcane) ->
    NameStr = lists:flatten(wfcli_forma_plan:to_list(Name)),
    case maps:get(rank, Arcane, undefined) of
        undefined -> NameStr;
        null -> NameStr;
        Rank -> lists:flatten(io_lib:format("~s (rank ~p)", [NameStr, Rank]))
    end;
arcane_label(#{}) ->
    "";
arcane_label(Arcane) ->
    wfcli_forma_plan:to_list(Arcane).

sort_plan(Plan) ->
    lists:sort(fun slot_order/2, maps:to_list(Plan)).

slot_order(aura, aura) -> true;
slot_order(aura, _) -> true;
slot_order(_, aura) -> false;
slot_order(exilus, exilus) -> true;
slot_order(exilus, _) -> true;
slot_order(_, exilus) -> false;
slot_order(A, B) when is_integer(A), is_integer(B) -> A =< B;
slot_order(A, _B) when is_integer(A) -> true;
slot_order(_, _) -> true.

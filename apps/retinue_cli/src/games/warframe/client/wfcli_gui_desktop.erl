%%%-------------------------------------------------------------------
%% XDG desktop launcher installation for wfgui.
%%%-------------------------------------------------------------------
-module(wfcli_gui_desktop).

-export([install/0, uninstall/0, status/0, path_report/0]).

-ifdef(TEST).
-export([desktop_entry/1, paths/1]).
-endif.

-define(DESKTOP_FILE, "wfgui.desktop").
-define(ICON_NAME, "wfgui").
-define(ICON_SIZES, [16, 24, 32, 48, 64, 128, 256, 512]).
-define(COMMAND_TIMEOUT, 30000).

-doc "Install the wfgui desktop entry and icon set into XDG_DATA_HOME.".
-spec install() -> {ok, map()} | {error, term()}.
install() ->
    case {paths(), gui_binary(), icon_source_dir()} of
        {{ok, Paths}, {ok, Gui}, {ok, IconSourceDir}} ->
            install(Paths, Gui, IconSourceDir);
        {{error, _Reason} = Error, _, _} -> Error;
        {_, {error, _Reason} = Error, _} -> Error;
        {_, _, {error, _Reason} = Error} -> Error
    end.

-doc "Remove the wfgui desktop entry and installed icons.".
-spec uninstall() -> {ok, map()} | {error, term()}.
uninstall() ->
    case paths() of
        {ok, Paths} ->
            Files = [maps:get(desktop, Paths) | maps:get(icons, Paths)],
            case delete_files(Files) of
                ok -> {ok, Paths#{installed => false}};
                {error, _Reason} = Error -> Error
            end;
        {error, _Reason} = Error -> Error
    end.

-doc "Return desktop paths and whether the launcher and icon set are installed.".
-spec status() -> {ok, map()} | {error, term()}.
status() ->
    case paths() of
        {ok, Paths} ->
            Installed = filelib:is_regular(maps:get(desktop, Paths)) andalso
                        lists:all(fun filelib:is_regular/1,
                                  maps:get(icons, Paths)),
            {ok, Paths#{installed => Installed}};
        {error, _Reason} = Error -> Error
    end.

-doc "Run wfgui's headless path reporter and return its JSON output.".
-spec path_report() -> {ok, binary()} | {error, term()}.
path_report() ->
    case gui_binary() of
        {ok, Gui} -> run_path_report(Gui);
        {error, _Reason} = Error -> Error
    end.

install(Paths, Gui, IconSourceDir) ->
    Desktop = maps:get(desktop, Paths),
    Copies = lists:zip(icon_sources(IconSourceDir), maps:get(icons, Paths)),
    case copy_files(Copies) of
        ok ->
            case filelib:ensure_dir(Desktop) of
                ok ->
                    case file:write_file(Desktop, desktop_entry(Gui)) of
                        ok -> {ok, Paths#{installed => true, executable => Gui}};
                        {error, Reason} -> {error, {desktop_write_failed, Desktop, Reason}}
                    end;
                {error, Reason} -> {error, {directory_create_failed, Desktop, Reason}}
            end;
        {error, _Reason} = Error -> Error
    end.

paths() ->
    case data_home() of
        {ok, Home} -> {ok, paths(Home)};
        {error, _Reason} = Error -> Error
    end.

paths(Home) ->
    IconRoot = filename:join([Home, "icons", "hicolor"]),
    Svg = filename:join([IconRoot, "scalable", "apps", ?ICON_NAME ++ ".svg"]),
    Pngs = [filename:join([IconRoot, integer_to_list(Size) ++ "x" ++
                          integer_to_list(Size), "apps", ?ICON_NAME ++ ".png"])
            || Size <- ?ICON_SIZES],
    #{desktop => filename:join([Home, "applications", ?DESKTOP_FILE]),
      icon => Svg,
      icons => [Svg | Pngs]}.

data_home() ->
    case os:getenv("XDG_DATA_HOME") of
        Value when is_list(Value), Value =/= "" -> {ok, filename:absname(Value)};
        _ ->
            case os:getenv("HOME") of
                Home when is_list(Home), Home =/= "" ->
                    {ok, filename:join(Home, ".local/share")};
                _ -> {error, home_directory_unavailable}
            end
    end.

gui_binary() ->
    find_regular(gui_binary_candidates(), gui_binary_not_found).

gui_binary_candidates() ->
    environment_path("WFGUI_BIN") ++
    [filename:join([Root, "bin", "wfgui"])
     || Root <- [retinue_build:update_root(), retinue_build:install_root()]] ++
    executable_path("wfgui").

icon_source_dir() ->
    Candidates = environment_path("WFGUI_ICON_DIR") ++
        [filename:join([Root, "share", "retinue", "games", "warframe", "icons"])
         || Root <- [retinue_build:update_root(), retinue_build:install_root()]],
    case [Dir || Dir <- Candidates,
                 lists:all(fun filelib:is_regular/1, icon_sources(Dir))] of
        [Dir | _] -> {ok, Dir};
        [] -> {error, {gui_icons_not_found, Candidates}}
    end.

icon_sources(Dir) ->
    [filename:join(Dir, ?ICON_NAME ++ ".svg") |
     [filename:join(Dir, ?ICON_NAME ++ "-" ++ integer_to_list(Size) ++ ".png")
      || Size <- ?ICON_SIZES]].

environment_path(Name) ->
    case os:getenv(Name) of
        Value when is_list(Value), Value =/= "" -> [filename:absname(Value)];
        _ -> []
    end.

executable_path(Name) ->
    case os:find_executable(Name) of false -> []; Path -> [Path] end.

find_regular(Paths, Error) ->
    case [filename:absname(Path) || Path <- Paths, filelib:is_regular(Path)] of
        [Path | _] -> {ok, Path};
        [] -> {error, {Error, Paths}}
    end.

run_path_report(Gui) ->
    try
        Port = open_port(
                 {spawn_executable, Gui},
                 [binary, exit_status, stderr_to_stdout,
                  {args, ["--paths-json"]}]),
        collect_path_report(Port, [])
    catch
        Class:Reason -> {error, {gui_start_failed, Class, Reason}}
    end.

collect_path_report(Port, Acc) ->
    receive
        {Port, {data, Data}} -> collect_path_report(Port, [Data | Acc]);
        {Port, {exit_status, 0}} ->
            {ok, iolist_to_binary(lists:reverse(Acc))};
        {Port, {exit_status, Status}} ->
            {error, {gui_exit_status, Status,
                     iolist_to_binary(lists:reverse(Acc))}}
    after ?COMMAND_TIMEOUT ->
        erlang:port_close(Port),
        {error, gui_command_timeout}
    end.

copy_files([]) -> ok;
copy_files([{Source, Target} | Rest]) ->
    case {filelib:ensure_dir(Target), file:read_file(Source)} of
        {ok, {ok, Data}} ->
            case file:write_file(Target, Data) of
                ok -> copy_files(Rest);
                {error, Reason} -> {error, {icon_write_failed, Target, Reason}}
            end;
        {{error, Reason}, _} -> {error, {directory_create_failed, Target, Reason}};
        {_, {error, Reason}} -> {error, {icon_read_failed, Source, Reason}}
    end.

delete_files([]) -> ok;
delete_files([Path | Rest]) ->
    case file:delete(Path) of
        ok -> delete_files(Rest);
        {error, enoent} -> delete_files(Rest);
        {error, Reason} -> {error, {delete_failed, Path, Reason}}
    end.

desktop_entry(Gui) ->
    iolist_to_binary([
        "[Desktop Entry]\n",
        "Type=Application\n",
        "Name=wfgui\n",
        "Comment=Warframe companion and planner\n",
        "Exec=", quote_exec(Gui), "\n",
        "Icon=wfgui\n",
        "Terminal=false\n",
        "Categories=Game;\n",
        "Keywords=Warframe;relics;inventory;mastery;\n",
        "StartupNotify=true\n",
        "StartupWMClass=wfgui\n",
        "X-GNOME-UsesNotifications=true\n"
    ]).

quote_exec(Path) -> [$", quote_exec_chars(Path), $"].

quote_exec_chars([]) -> [];
quote_exec_chars([Char | Rest])
  when Char =:= $\\; Char =:= $"; Char =:= $`; Char =:= $$ ->
    [$\\, Char | quote_exec_chars(Rest)];
quote_exec_chars([Char | Rest]) ->
    [Char | quote_exec_chars(Rest)].

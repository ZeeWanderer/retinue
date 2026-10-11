-module(wfcli_companion_cli).

-export([command/0]).
-import(retinue_cli_args, [flag/3]).

-define(COMPANION_RECONNECT_RETRIES, 30).
-define(COMPANION_RECONNECT_DELAY_MS, 100).

-ifdef(TEST).
-export([capture_directory/1, format_local/1, preview_directory/1,
         retry_companion_command/3]).
-endif.

command() ->
    #{help => "manage native game companion",
      commands => #{
        "status" => #{help => "show connections, collectors and metadata cache", handler => fun(_) -> status() end},
        "start" => #{help => "start companion service", handler => fun(_) -> start() end},
        "stop" => #{help => "stop companion service", handler => fun(_) -> stop() end},
        "restart" => #{help => "restart companion service", handler => fun(_) -> restart() end},
        "show" => visibility("overlay", <<"overlay">>, true),
        "hide" => visibility("overlay", <<"overlay">>, false),
        "hud" => #{help => "control diagnostic HUD",
                   commands => #{"show" => visibility("HUD", <<"hud">>, true),
                                 "hide" => visibility("HUD", <<"hud">>, false)}},
        "probe" => #{help => "probe capture and OCR dependencies",
                     handler => fun(_) -> diagnostic(["probe"]) end},
        "paths" => #{help => "show companion XDG directories",
                     handler => fun(_) -> retinue_path_cli:run(#{owner => wfcompanion}) end},
        "screenshot" => #{help => "capture Warframe", handler => fun screenshot/1,
                          arguments => [#{name => path, required => false, help => "output PNG"}]},
        "relic-ocr" => #{help => "test relic OCR", handler => fun(Args) ->
                            diagnostic(["relic-ocr" | maps:get(native_args, Args, [])]) end,
                         arguments => [#{name => native_args, nargs => all, required => false,
                                         help => "native relic-ocr arguments"}]},
        "capture" => #{help => "capture event-triggered memory evidence",
                       commands => #{
                         "status" => #{help => "show armed capture and last result",
                                       handler => fun(_) -> capture_status() end},
                         "arm" => #{help => "arm a capture", commands => #{
                            "relic-reward" => #{help => "capture the next relic reward screen",
                                handler => fun arm_capture/1,
                                arguments => [#{name => directory, required => false,
                                                help => "evidence directory"}]}}},
                         "cancel" => #{help => "cancel armed or running captures", handler => fun(_) ->
                             send_capture_command(
                               #{<<"command">> => <<"capture">>, <<"action">> => <<"cancel">>,
                                 <<"target">> => <<"relic_reward">>},
                               "relic-reward evidence capture cancelled") end}}},
        "preview" => #{help => "render overlay previews",
                       commands => #{
                         "list" => #{help => "list preview types",
                                      arguments => [flag(animated, "animated", "list animated types")],
                                      handler => fun(Args) ->
                                          Extra = case maps:get(animated, Args, false) of
                                                      true -> ["--animated"]; false -> []
                                                  end,
                                          diagnostic(["preview", "list" | Extra]) end},
                         "image" => preview_command("image"),
                         "video" => preview_command("video")}},
        "logs" => wfcli_diagnostics_cli:logs_command(companion),
        "diagnostics" => wfcli_companion_diagnostics_cli:command(),
        "install" => #{help => "configure Steam launch options", handler => fun install/1,
                       arguments => [flag(dry_run, "dry-run", "show planned changes")]},
        "uninstall" => #{help => "restore Steam launch options", handler => fun uninstall/1,
                         arguments => [flag(dry_run, "dry-run", "show planned changes")]}}}.

visibility(Label, Name, Visible) ->
    Verb = case Visible of true -> "enable "; false -> "disable " end,
    Target = case Name of <<"overlay">> -> "the entire overlay"; _ -> Label end,
    #{help => Verb ++ Target, handler => fun(_) -> set_visibility(Name, Label, Visible) end}.

preview_command(Mode) ->
    #{help => "render " ++ Mode ++ " previews", handler => fun(Args) -> preview(Mode, Args) end,
      arguments => [#{name => target, help => "preview type or all", completion => ["all"]},
                    #{name => path, required => false, help => "output path"}]}.

status() ->
    Managed = wfcli_companion_process:unit_active(),
    case daemon_status() of
        {running, Info} ->
            Local = maps:get(local_api, Info, unavailable),
            Player = maps:get(player, Info, unavailable),
            retinue_output:emit(#{managed_service => Managed, daemon => running,
                                connection => Local, player => Player,
                                game_metadata => maps:get(game_metadata, Info, unavailable)}, fun() ->
            io:format("wfcompanion~n  wfcli-managed service: ~s~n",
                      [case Managed of true -> "active"; false -> "inactive" end]),
            print_local(Local),
            print_player(Player),
            io:put_chars(wfcli_companion_format:collectors(Player, Local)),
            io:put_chars(wfcli_companion_format:metadata(maps:get(game_metadata, Info, unavailable))),
            io:put_chars(wfcli_companion_format:captures(Player, Local))
            end);
        stopped -> retinue_output:emit(#{managed_service => Managed, daemon => stopped},
                       fun() -> io:format("retinue-daemon stopped; no companion connection state~n") end);
        {error, Reason} -> fail(retinue_client:format_error(Reason))
    end.

capture_status() ->
    case daemon_status() of
        {running, Info} ->
            retinue_output:emit(#{player => maps:with([capture, capture_result], maps:get(player, Info, #{})),
                                connection => maps:get(local_api, Info, unavailable)}, fun() ->
            io:put_chars(wfcli_companion_format:captures(
                maps:get(player, Info, unavailable), maps:get(local_api, Info, unavailable)))
            end);
        stopped -> retinue_output:emit(#{daemon => stopped},
                       fun() -> io:format("retinue-daemon stopped; capture state unavailable~n") end);
        {error, Reason} -> fail(retinue_client:format_error(Reason))
    end.

start() ->
    case connected_companions() of
        {ok, [_ | _] = Details} ->
            retinue_output:emit(#{state => already_connected, companions => Details}, fun() ->
            io:format("wfcompanion already connected~n"),
            print_companions(Details) end);
        {ok, []} ->
            case wfcli_companion_process:start() of
                {ok, already_running, _Output} ->
                    retinue_output:emit(#{state => already_running},
                        fun() -> io:format("wfcompanion managed service already active~n") end);
                {ok, started, Output} ->
                    retinue_output:emit(#{state => started, output => Output}, fun() ->
                        io:format("wfcompanion started as user service~n"),
                        print_command_output(Output) end);
                {error, Reason} -> fail(format_process_error(Reason))
            end;
        {error, Reason} -> fail(retinue_client:format_error(Reason))
    end.

stop() ->
    case stop_result() of
        {ok, stopped, Output} ->
            retinue_output:emit(#{state => stopped, output => Output}, fun() ->
            io:format("wfcompanion stopped~n"),
            print_command_output(Output) end);
        {ok, not_running, _Output} -> retinue_output:emit(#{state => not_running},
            fun() -> io:format("wfcompanion not running~n") end);
        {error, Reason} -> fail(format_process_error(Reason))
    end.

restart() ->
    case stop_result() of
        {ok, _State, _Output} -> start();
        {error, Reason} -> fail(format_process_error(Reason))
    end.

stop_result() ->
    case connected_companions() of
        {ok, Details} ->
            Modes = [maps:get(mode, Detail, <<"unknown">>) || Detail <- Details],
            case lists:member(<<"launch">>, Modes) of
                true -> {error, steam_launch_companion};
                false -> stop_managed_or_refuse(Details)
            end;
        {error, Reason} -> {error, Reason}
    end.

stop_managed_or_refuse(Details) ->
    case wfcli_companion_process:unit_active() of
        true ->
            case wfcli_companion_process:stop() of
                {ok, Output} -> {ok, stopped, Output};
                {error, _Reason} = Error -> Error
            end;
        false when Details =:= [] -> {ok, not_running, <<>>};
        false -> {error, unmanaged_companion}
    end.

set_visibility(CommandName, Label, Visible) ->
    Command = #{<<"command">> => CommandName, <<"visible">> => Visible},
    case companion_command(Command) of
        {ok, {ok, 0}} -> fail("no wfcompanion is connected");
        {ok, {ok, Count}} ->
            State = case Visible of true -> "shown"; false -> "hidden" end,
            retinue_output:emit(#{command => CommandName, visible => Visible, companions => Count},
                fun() -> io:format("~s ~s on ~p companion(s)~n", [Label, State, Count]) end);
        {error, Reason} -> fail(retinue_client:format_error(Reason))
    end.

print_local(unavailable) -> io:format("wfcompanion API unavailable~n");
print_local(Local) ->
    io:put_chars(format_local(Local)),
    print_companions(maps:get(companion_details, Local, [])).

format_local(Local) ->
    Contract = maps:get(contract, Local),
    Envelope = maps:get(<<"envelope">>, Contract),
    Interfaces = maps:get(<<"interfaces">>, Contract),
    ["wfcompanion API\n",
     io_lib:format("  socket: ~s~n", [maps:get(socket, Local)]),
     io_lib:format("  handshake envelope: ~p~n", [Envelope]),
     io_lib:format("  interfaces: ~s~n", [format_interfaces(Interfaces)]),
     io_lib:format("  connections: ~p~n", [maps:get(connections, Local)]),
     io_lib:format("  companions: ~p~n", [maps:get(companions, Local)])].

format_interfaces(Interfaces) ->
    string:join(
      [binary_to_list(Name) ++ "=" ++ integer_to_list(Version)
       || {Name, Version} <- lists:sort(maps:to_list(Interfaces))],
      ", ").

print_companions([]) -> ok;
print_companions(Details) ->
    lists:foreach(
      fun(Detail) ->
          io:format("    pid: ~p, mode: ~ts, version: ~ts~n",
                    [maps:get(os_pid, Detail, undefined),
                     maps:get(mode, Detail, <<"unknown">>),
                     maps:get(version, Detail, <<"unknown">>)])
      end,
      Details).

print_player(unavailable) -> ok;
print_player(Player) ->
    io:format("  player revision: ~p~n", [maps:get(revision, Player, 0)]),
    io:format("  player sources: ~s~n",
              [string:join([retinue_text:to_list(S) || S <- maps:get(sources, Player, [])], ", ")]),
    io:format("  game active: ~p~n", [maps:get(game_active, Player, false)]).

daemon_status() ->
    case retinue_client:status() of
        {running, _Node, Info} ->
            {running, maps:merge(Info, maps:get(warframe, maps:get(games, Info, #{}), #{}))};
        {stopped, _Node} -> stopped;
        {error, _Reason} = Error -> Error
    end.

connected_companions() ->
    case daemon_status() of
        {running, #{local_api := Local}} when is_map(Local) ->
            {ok, maps:get(companion_details, Local, [])};
        {running, _Info} -> {ok, []};
        stopped -> {ok, []};
        {error, _Reason} = Error -> Error
    end.

screenshot(Args) ->
    Path = maps:get(path, Args, wfcli_paths:cache_file("companion-screenshot.png")),
    ok = filelib:ensure_dir(Path),
    diagnostic(["screenshot", Path]).

arm_capture(Args) ->
    Directory = maps:get(directory, Args, capture_directory(erlang:system_time(millisecond))),
    Output = filename:absname(Directory),
    send_capture_command(
      #{<<"command">> => <<"capture">>, <<"action">> => <<"arm">>,
        <<"target">> => <<"relic_reward">>,
        <<"directory">> => unicode:characters_to_binary(Output),
        <<"timeout_ms">> => 30 * 60 * 1000},
      io_lib:format("relic-reward evidence capture requested~n  output: ~s", [Output])).

capture_directory(Timestamp) ->
    wfcli_paths:cache_file(
      filename:join("captures", "relic-reward-" ++ integer_to_list(Timestamp))).

send_capture_command(Command, Message) ->
    case companion_command(Command) of
        {ok, {ok, 0}} -> fail("no wfcompanion is connected");
        {ok, {ok, Count}} -> retinue_output:emit(#{command => Command, companions => Count},
            fun() -> io:format("~ts on ~p companion(s)~n", [Message, Count]) end);
        {error, Reason} -> fail(retinue_client:format_error(Reason))
    end.

companion_command(Command) ->
    Call = fun() -> retinue_client:call(warframe, {companion_command, Command}) end,
    retry_companion_command(
      Call, ?COMPANION_RECONNECT_RETRIES, ?COMPANION_RECONNECT_DELAY_MS).

retry_companion_command(Call, Attempts, Delay) ->
    case Call() of
        {ok, {ok, 0}} when Attempts > 0 ->
            timer:sleep(Delay),
            retry_companion_command(Call, Attempts - 1, Delay);
        Result -> Result
    end.

preview(Mode, #{target := Target} = Args) ->
    Extension = case Mode of "image" -> ".png"; "video" -> ".webm" end,
    Default = case Target of
                  "all" -> default_preview_directory();
                  _ -> filename:join(default_preview_directory(), Target ++ Extension)
              end,
    diagnostic(["preview", Mode, Target, maps:get(path, Args, Default)]).

default_preview_directory() ->
    case retinue_native_program:binary(wfcompanion) of
        {ok, Companion} -> preview_directory(Companion);
        {error, _Reason} -> filename:absname("previews")
    end.

preview_directory(Companion) ->
    BinDir = filename:dirname(filename:absname(Companion)),
    Parent = filename:dirname(BinDir),
    Root = case filename:basename(Parent) of
               Name when Name =:= "build"; Name =:= "dev"; Name =:= "prod" ->
                   filename:dirname(Parent);
               _ -> Parent
           end,
    filename:join(Root, "previews").

diagnostic(Args) ->
    case retinue_native_program:run(wfcompanion, Args) of
        {ok, Output} ->
            Data = case retinue_json:decode(Output) of
                {ok, Value} -> {json, Value};
                _ -> #{output => Output}
            end,
            retinue_output:emit(Data, fun() -> print_command_output(Output) end);
        {error, Reason} -> fail(format_process_error(Reason))
    end.

install(Args) ->
    case retinue_native_program:binary(wfcompanion) of
        {ok, Companion} ->
            print_steam_result(
              wfcli_companion_steam:install(Companion, maps:get(dry_run, Args, false)), install);
        {error, Reason} -> fail(format_process_error(Reason))
    end.

uninstall(Args) ->
    print_steam_result(wfcli_companion_steam:uninstall(maps:get(dry_run, Args, false)), uninstall).

print_steam_result({ok, Result}, Action) ->
    retinue_output:emit(Result#{action => Action}, fun() -> print_steam_details(Result, Action) end);
print_steam_result({error, Reason}, _Action) -> fail(format_steam_error(Reason)).

print_steam_details(Result, install) ->
    Action = case maps:get(dry_run, Result) of true -> "would install"; false -> "installed" end,
    io:format("wfcompanion ~s for Warframe~n", [Action]),
    io:format("  config: ~ts~n", [maps:get(config, Result)]),
    io:format("  previous: ~ts~n", [maps:get(current, Result)]),
    io:format("  launch options: ~ts~n", [maps:get(proposed, Result)]);
print_steam_details(Result, uninstall) ->
    Action = case maps:get(dry_run, Result) of true -> "would uninstall"; false -> "uninstalled" end,
    io:format("wfcompanion ~s from Warframe~n", [Action]),
    io:format("  config: ~ts~n", [maps:get(config, Result)]),
    io:format("  restored: ~ts~n", [maps:get(original, Result)]).

format_steam_error(steam_running) ->
    "Steam is running; close Steam before changing launch options";
format_steam_error(companion_not_installed) -> "wfcompanion Steam wrapper is not installed";
format_steam_error(warframe_steam_config_not_found) ->
    "Warframe Steam configuration was not found";
format_steam_error({multiple_warframe_steam_configs, Paths}) ->
    io_lib:format("multiple Steam users contain Warframe: ~p", [Paths]);
format_steam_error({launch_options_changed, Current}) ->
    io_lib:format("Warframe launch options changed after install; refusing overwrite: ~ts", [Current]);
format_steam_error(Reason) -> io_lib:format("Steam setup failed: ~p", [Reason]).

format_process_error(steam_launch_companion) ->
    "companion is Steam launch wrapper; stop it by exiting Warframe";
format_process_error(unmanaged_companion) ->
    "companion is connected but was not started by wfcli; refusing to kill it";
format_process_error({companion_binary_not_found, _Candidates}) ->
    "wfcompanion binary not found; run `make companion`";
format_process_error({exit_status, Status, Output}) ->
    io_lib:format("command exited with status ~p: ~ts", [Status, string:trim(Output)]);
format_process_error({companion_start_failed, Reason}) -> format_process_error(Reason);
format_process_error(Reason) -> io_lib:format("companion command failed: ~p", [Reason]).

print_command_output(<<>>) -> ok;
print_command_output(Output) ->
    io:put_chars(Output),
    case binary:last(Output) of $\n -> ok; _ -> io:put_chars("\n") end.

fail(Message) ->
    io:format(standard_error, "error: ~ts~n", [Message]),
    halt(1).

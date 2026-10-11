%%%-------------------------------------------------------------------
%% Native companion discovery and Warframe transient systemd lifecycle.
%%%-------------------------------------------------------------------
-module(retinue_native_program).

-export([binary/1, run/2, run_command/3, run_command_raw/3]).

-ifdef(TEST).
-export([binary_candidates/3]).
-endif.

-define(COMMAND_TIMEOUT, 120000).

-type companion() :: wfcompanion | poe2companion | arccompanion.


-doc "Find a native companion in the current installation or shell path.".
-spec binary(companion()) -> {ok, file:filename_all()} | {error, term()}.
binary(App) when App =:= wfcompanion; App =:= poe2companion; App =:= arccompanion ->
    Script = resolve_script_path(filename:absname(escript_name()), 8),
    Variable = case App of
        wfcompanion -> "WFCOMPANION_BIN";
        poe2companion -> "POE2COMPANION_BIN";
        arccompanion -> "ARCCOMPANION_BIN"
    end,
    Candidates = binary_candidates(App, Script, os:getenv(Variable)),
    case [Path || Path <- Candidates, filelib:is_regular(Path)] of
        [Path | _] -> {ok, Path};
        [] -> {error, {companion_binary_not_found, Candidates}}
    end.


-doc "Run a native companion command and collect output.".
-spec run(companion(), [string()]) -> {ok, binary()} | {error, term()}.
run(App, Args) ->
    case binary(App) of
        {ok, Path} -> run_command(Path, Args, []);
        {error, _Reason} = Error -> Error
    end.


binary_candidates(App, Script, Environment) ->
    Name = atom_to_list(App),
    ScriptDir = filename:dirname(Script),
    Env = case Environment of false -> []; "" -> []; Value -> [filename:absname(Value)] end,
    unique(Env ++ [
        filename:absname(filename:join(ScriptDir, Name)),
        filename:join([retinue_build:install_root(), "bin", Name]),
        filename:join([retinue_build:update_root(), "bin", Name])
    ] ++ path_candidate(Name)).

escript_name() ->
    try escript:script_name() of
        undefined -> filename:absname("retinue");
        Name -> Name
    catch
        _:_ -> filename:absname("retinue")
    end.

resolve_script_path(Path, 0) -> Path;
resolve_script_path(Path, Remaining) ->
    case file:read_link(Path) of
        {ok, Target} ->
            Next = case filename:pathtype(Target) of
                absolute -> Target;
                _ -> filename:join(filename:dirname(Path), Target)
            end,
            resolve_script_path(filename:absname(Next), Remaining - 1);
        {error, _Reason} -> Path
    end.

path_candidate(Name) ->
    case os:find_executable(Name) of
        false -> [];
        Path -> [Path]
    end.

unique(Paths) ->
    lists:reverse(
      element(1,
              lists:foldl(
                fun(Path, {Acc, Seen}) ->
                    case sets:is_element(Path, Seen) of
                        true -> {Acc, Seen};
                        false -> {[Path | Acc], sets:add_element(Path, Seen)}
                    end
                end,
                {[], sets:new()},
                Paths))).

run_command(Executable, Args, Environment) ->
    case run_command_raw(Executable, Args, Environment) of
        {0, Output} -> {ok, Output};
        {error, _Reason} = Error -> Error;
        {Status, Output} -> {error, {exit_status, Status, Output}}
    end.

run_command_raw(Executable, Args, Environment) ->
    try
        Port = open_port(
                 {spawn_executable, Executable},
                 [binary, exit_status, stderr_to_stdout, {args, Args}, {env, Environment}]),
        collect_port(Port, [])
    catch
        Class:Reason -> {error, {command_start_failed, Executable, Class, Reason}}
    end.

collect_port(Port, Acc) ->
    receive
        {Port, {data, Data}} -> collect_port(Port, [Data | Acc]);
        {Port, {exit_status, Status}} ->
            {Status, iolist_to_binary(lists:reverse(Acc))}
    after ?COMMAND_TIMEOUT ->
        erlang:port_close(Port),
        {error, command_timeout}
    end.

-module(wfcli_gui_cli).

-export([command/0]).

command() ->
    #{help => "manage the desktop launcher",
      commands => #{
        "install" => #{help => "install desktop launcher", handler => fun(_) ->
            print_result(install, wfcli_gui_desktop:install()) end},
        "status" => #{help => "show desktop launcher paths", handler => fun(_) ->
            print_result(status, wfcli_gui_desktop:status()) end},
        "uninstall" => #{help => "remove desktop launcher", handler => fun(_) ->
            print_result(uninstall, wfcli_gui_desktop:uninstall()) end}}}.

print_result(Action, {ok, Result}) ->
    retinue_output:emit(Result#{action => Action}, fun() -> print_details(Action, Result) end);
print_result(_Action, {error, Reason}) -> fail(io_lib:format("~p", [Reason])).

print_details(Action, Result) ->
    Installed = maps:get(installed, Result),
    State = case {Action, Installed} of
        {install, true} -> "installed";
        {uninstall, false} -> "uninstalled";
        {status, true} -> "installed";
        {status, false} -> "not installed"
    end,
    io:format("wfgui desktop launcher ~s~n", [State]),
    io:format("  desktop: ~ts~n", [maps:get(desktop, Result)]),
    io:format("  icon: ~ts~n", [maps:get(icon, Result)]),
    case maps:find(executable, Result) of
        {ok, Executable} -> io:format("  executable: ~ts~n", [Executable]);
        error -> ok
    end.

fail(Message) ->
    io:format(standard_error, "error: ~ts~n", [Message]),
    halt(1).
